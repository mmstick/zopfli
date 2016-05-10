/*
Copyright 2011 Google Inc. All Rights Reserved.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

Author: lode.vandevenne@gmail.com (Lode Vandevenne)
Author: jyrki.alakuijala@gmail.com (Jyrki Alakuijala)
*/

#include "squeeze.h"

#include <assert.h>
#include <math.h>
#include <stdio.h>

#include "blocksplitter.h"
#include "deflate.h"
#include "symbols.h"
#include "tree.h"
#include "util.h"

typedef struct SymbolStats SymbolStats;

extern SymbolStats* symbol_stats_new();

extern void CopyStats(SymbolStats* source, SymbolStats* dest);
extern void AddWeighedStatFreqs(const SymbolStats* stats1, double w1,
                                const SymbolStats* stats2, double w2,
                                SymbolStats* result);

typedef struct RanState RanState;

extern RanState* ran_state_new();
extern void RandomizeStatFreqs(RanState* state, SymbolStats* stats);
extern void ClearStatFreqs(SymbolStats* stats);
/*
Function that calculates a cost based on a model for the given LZ77 symbol.
litlen: means literal symbol if dist is 0, length otherwise.
*/
typedef double CostModelFun(unsigned litlen, unsigned dist, void* context);

extern double GetCostFixed(unsigned litlen, unsigned dist, void* unused);

extern double GetCostStat(unsigned litlen, unsigned dist, void* context);

static size_t zopfli_min(size_t a, size_t b) {
  return a < b ? a : b;
}

extern double GetCostModelMinCost(CostModelFun* costmodel, void* costcontext);
extern unsigned short ZopfliHashSameAt(const ZopfliHash* h, size_t index);


/*
Performs the forward pass for "squeeze". Gets the most optimal length to reach
every byte from a previous byte, using cost calculations.
s: the ZopfliBlockState
in: the input data array
instart: where to start
inend: where to stop (not inclusive)
costmodel: function to calculate the cost of some lit/len/dist pair.
costcontext: abstract context for the costmodel function
length_array: output array of size (inend - instart) which will receive the best
    length to reach this byte from a previous byte.
returns the cost that was, according to the costmodel, needed to get to the end.
*/
static double GetBestLengths(ZopfliBlockState *s,
                             const unsigned char* in,
                             size_t instart, size_t inend,
                             CostModelFun* costmodel, void* costcontext,
                             unsigned short* length_array,
                             ZopfliHash* h, float* costs) {
  /* Best cost to get here so far. */
  size_t blocksize = inend - instart;
  size_t i = 0, k, kend;
  unsigned short leng;
  unsigned short dist;
  LongestMatch longest_match;
  unsigned short sublen[259];
  size_t windowstart = instart > ZOPFLI_WINDOW_SIZE
      ? instart - ZOPFLI_WINDOW_SIZE : 0;
  double result;
  double mincost = GetCostModelMinCost(costmodel, costcontext);
  double mincostaddcostj;

  if (instart == inend) return 0;

  ZopfliResetHash(ZOPFLI_WINDOW_SIZE, h);
  ZopfliWarmupHash(in, windowstart, inend, h);
  for (i = windowstart; i < instart; i++) {
    ZopfliUpdateHash(in, i, inend, h);
  }

  for (i = 1; i < blocksize + 1; i++) costs[i] = ZOPFLI_LARGE_FLOAT;
  costs[0] = 0;  /* Because it's the start. */
  length_array[0] = 0;

  for (i = instart; i < inend; i++) {
    size_t j = i - instart;  /* Index in the costs array and length_array. */
    ZopfliUpdateHash(in, i, inend, h);

    /* If we're in a long repetition of the same character and have more than
    ZOPFLI_MAX_MATCH characters before and after our position. */
    if (ZopfliHashSameAt(h, i & ZOPFLI_WINDOW_MASK) > ZOPFLI_MAX_MATCH * 2
        && i > instart + ZOPFLI_MAX_MATCH + 1
        && i + ZOPFLI_MAX_MATCH * 2 + 1 < inend
        && ZopfliHashSameAt(h, (i - ZOPFLI_MAX_MATCH) & ZOPFLI_WINDOW_MASK)
            > ZOPFLI_MAX_MATCH) {
      double symbolcost = costmodel(ZOPFLI_MAX_MATCH, 1, costcontext);
      /* Set the length to reach each one to ZOPFLI_MAX_MATCH, and the cost to
      the cost corresponding to that length. Doing this, we skip
      ZOPFLI_MAX_MATCH values to avoid calling ZopfliFindLongestMatch. */
      for (k = 0; k < ZOPFLI_MAX_MATCH; k++) {
        costs[j + ZOPFLI_MAX_MATCH] = costs[j] + symbolcost;
        length_array[j + ZOPFLI_MAX_MATCH] = ZOPFLI_MAX_MATCH;
        i++;
        j++;
        ZopfliUpdateHash(in, i, inend, h);
      }
    }

    longest_match = ZopfliFindLongestMatch(s, h, in, i, inend, ZOPFLI_MAX_MATCH, sublen);
    dist = longest_match.distance;
    leng = longest_match.length;

    /* Literal. */
    if (i + 1 <= inend) {
      double newCost = costmodel(in[i], 0, costcontext) + costs[j];
      assert(newCost >= 0);
      if (newCost < costs[j + 1]) {
        costs[j + 1] = newCost;
        length_array[j + 1] = 1;
      }
    }
    /* Lengths. */
    kend = zopfli_min(leng, inend-i);
    mincostaddcostj = mincost + costs[j];
    for (k = 3; k <= kend; k++) {
      double newCost;

      /* Calling the cost model is expensive, avoid this if we are already at
      the minimum possible cost that it can return. */
     if (costs[j + k] <= mincostaddcostj) continue;

      newCost = costmodel(k, sublen[k], costcontext) + costs[j];
      assert(newCost >= 0);
      if (newCost < costs[j + k]) {
        assert(k <= ZOPFLI_MAX_MATCH);
        costs[j + k] = newCost;
        length_array[j + k] = k;
      }
    }
  }

  assert(costs[blocksize] >= 0);
  result = costs[blocksize];

  return result;
}

/*
Calculates the optimal path of lz77 lengths to use, from the calculated
length_array. The length_array must contain the optimal length to reach that
byte. The path will be filled with the lengths to use, so its data size will be
the amount of lz77 symbols.
*/
static void TraceBackwards(size_t size, const unsigned short* length_array,
                           unsigned short** path, size_t* pathsize) {
  size_t index = size;
  if (size == 0) return;
  for (;;) {
    ZOPFLI_APPEND_DATA(length_array[index], path, pathsize);
    assert(length_array[index] <= index);
    assert(length_array[index] <= ZOPFLI_MAX_MATCH);
    assert(length_array[index] != 0);
    index -= length_array[index];
    if (index == 0) break;
  }

  /* Mirror result. */
  for (index = 0; index < *pathsize / 2; index++) {
    unsigned short temp = (*path)[index];
    (*path)[index] = (*path)[*pathsize - index - 1];
    (*path)[*pathsize - index - 1] = temp;
  }
}

static void FollowPath(ZopfliBlockState* s,
                       const unsigned char* in, size_t instart, size_t inend,
                       unsigned short* path, size_t pathsize,
                       ZopfliLZ77Store* store, ZopfliHash *h) {
  size_t i, j, pos = 0;
  size_t windowstart = instart > ZOPFLI_WINDOW_SIZE
      ? instart - ZOPFLI_WINDOW_SIZE : 0;

  size_t total_length_test = 0;

  if (instart == inend) return;

  ZopfliResetHash(ZOPFLI_WINDOW_SIZE, h);
  ZopfliWarmupHash(in, windowstart, inend, h);
  for (i = windowstart; i < instart; i++) {
    ZopfliUpdateHash(in, i, inend, h);
  }

  pos = instart;
  for (i = 0; i < pathsize; i++) {
    unsigned short length = path[i];
    unsigned short dummy_length;
    unsigned short dist;
    LongestMatch longest_match;
    assert(pos < inend);

    ZopfliUpdateHash(in, pos, inend, h);

    /* Add to output. */
    if (length >= ZOPFLI_MIN_MATCH) {
      /* Get the distance by recalculating longest match. The found length
      should match the length from the path. */
      longest_match = ZopfliFindLongestMatch(s, h, in, pos, inend, length, 0);
      dist = longest_match.distance;
      dummy_length = longest_match.length;
      assert(!(dummy_length != length && length > 2 && dummy_length > 2));
      ZopfliVerifyLenDist(in, inend, pos, dist, length);
      ZopfliStoreLitLenDist(length, dist, pos, store);
      total_length_test += length;
    } else {
      length = 1;
      ZopfliStoreLitLenDist(in[pos], 0, pos, store);
      total_length_test++;
    }


    assert(pos + length <= inend);
    for (j = 1; j < length; j++) {
      ZopfliUpdateHash(in, pos + j, inend, h);
    }

    pos += length;
  }
}

extern void CalculateStatistics(SymbolStats* stats);
extern void GetStatistics(const ZopfliLZ77Store* store, SymbolStats* stats);

/*
Does a single run for ZopfliLZ77Optimal. For good compression, repeated runs
with updated statistics should be performed.
s: the block state
in: the input data array
instart: where to start
inend: where to stop (not inclusive)
path: pointer to dynamically allocated memory to store the path
pathsize: pointer to the size of the dynamic path array
length_array: array of size (inend - instart) used to store lengths
costmodel: function to use as the cost model for this squeeze run
costcontext: abstract context for the costmodel function
store: place to output the LZ77 data
returns the cost that was, according to the costmodel, needed to get to the end.
    This is not the actual cost.
*/
static double LZ77OptimalRun(ZopfliBlockState* s,
    const unsigned char* in, size_t instart, size_t inend,
    unsigned short** path, size_t* pathsize,
    unsigned short* length_array, CostModelFun* costmodel,
    void* costcontext, ZopfliLZ77Store* store,
    ZopfliHash* h, float* costs) {
  double cost = GetBestLengths(s, in, instart, inend, costmodel,
                costcontext, length_array, h, costs);
  free(*path);
  *path = 0;
  *pathsize = 0;
  TraceBackwards(inend - instart, length_array, path, pathsize);
  FollowPath(s, in, instart, inend, *path, *pathsize, store, h);
  assert(cost < ZOPFLI_LARGE_FLOAT);
  return cost;
}

void ZopfliLZ77Optimal(ZopfliBlockState *s,
                       const unsigned char* in, size_t instart, size_t inend,
                       int numiterations,
                       ZopfliLZ77Store* store) {
  /* Dist to get to here with smallest cost. */
  size_t blocksize = inend - instart;
  unsigned short* length_array =
      (unsigned short*)malloc(sizeof(unsigned short) * (blocksize + 1));
  unsigned short* path = 0;
  size_t pathsize = 0;
  ZopfliLZ77Store currentstore;
  ZopfliHash* h = ZopfliInitHash(ZOPFLI_WINDOW_SIZE);
  SymbolStats *stats, *beststats, *laststats;
  int i;
  float* costs = (float*)malloc(sizeof(float) * (blocksize + 1));
  double cost;
  double bestcost = ZOPFLI_LARGE_FLOAT;
  double lastcost = 0;
  /* Try randomizing the costs a bit once the size stabilizes. */
  RanState *ran_state;
  int lastrandomstep = -1;

  if (!costs) exit(-1); /* Allocation failed. */
  if (!length_array) exit(-1); /* Allocation failed. */

  ran_state = ran_state_new();
  stats = symbol_stats_new();
  beststats = symbol_stats_new();
  laststats = symbol_stats_new();
  ZopfliInitLZ77Store(in, &currentstore);

  /* Do regular deflate, then loop multiple shortest path runs, each time using
  the statistics of the previous run. */

  /* Initial run. */
  ZopfliLZ77Greedy(s, in, instart, inend, &currentstore, h);
  GetStatistics(&currentstore, stats);

  /* Repeat statistics with each time the cost model from the previous stat
  run. */
  for (i = 0; i < numiterations; i++) {
    ZopfliCleanLZ77Store(&currentstore);
    ZopfliInitLZ77Store(in, &currentstore);
    LZ77OptimalRun(s, in, instart, inend, &path, &pathsize,
                   length_array, GetCostStat, (void*)stats,
                   &currentstore, h, costs);
    cost = ZopfliCalculateBlockSize(&currentstore, 0, currentstore.size, 2);
    if (s->options->verbose_more || (s->options->verbose && cost < bestcost)) {
      fprintf(stderr, "Iteration %d: %d bit\n", i, (int) cost);
    }
    if (cost < bestcost) {
      /* Copy to the output store. */
      ZopfliCopyLZ77Store(&currentstore, store);
      CopyStats(stats, beststats);
      bestcost = cost;
    }
    CopyStats(stats, laststats);
    ClearStatFreqs(stats);
    GetStatistics(&currentstore, stats);
    if (lastrandomstep != -1) {
      /* This makes it converge slower but better. Do it only once the
      randomness kicks in so that if the user does few iterations, it gives a
      better result sooner. */
      AddWeighedStatFreqs(stats, 1.0, laststats, 0.5, stats);
      CalculateStatistics(stats);
    }
    if (i > 5 && cost == lastcost) {
      CopyStats(beststats, stats);
      RandomizeStatFreqs(ran_state, stats);
      CalculateStatistics(stats);
      lastrandomstep = i;
    }
    lastcost = cost;
  }

  free(length_array);
  free(path);
  free(costs);
  ZopfliCleanLZ77Store(&currentstore);
  ZopfliCleanHash(h);
}

void ZopfliLZ77OptimalFixed(ZopfliBlockState *s,
                            const unsigned char* in,
                            size_t instart, size_t inend,
                            ZopfliLZ77Store* store)
{
  /* Dist to get to here with smallest cost. */
  size_t blocksize = inend - instart;
  unsigned short* length_array =
      (unsigned short*)malloc(sizeof(unsigned short) * (blocksize + 1));
  unsigned short* path = 0;
  size_t pathsize = 0;
  ZopfliHash* h = ZopfliInitHash(ZOPFLI_WINDOW_SIZE);
  float* costs = (float*)malloc(sizeof(float) * (blocksize + 1));

  if (!costs) exit(-1); /* Allocation failed. */
  if (!length_array) exit(-1); /* Allocation failed. */

  s->blockstart = instart;
  s->blockend = inend;

  /* Shortest path for fixed tree This one should give the shortest possible
  result for fixed tree, no repeated runs are needed since the tree is known. */
  LZ77OptimalRun(s, in, instart, inend, &path, &pathsize,
                 length_array, GetCostFixed, 0, store, h, costs);

  free(length_array);
  free(path);
  free(costs);
  ZopfliCleanHash(h);
}
