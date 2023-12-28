/// @file
/// @brief Routines for building a Doom map's BLOCKMAP lump.

#pragma once

#include "doomdata.hpp"
#include "tarray.hpp"

class FBlockmapBuilder {
public:
	FBlockmapBuilder(FLevel& level);
	WORD* GetBlockmap(int32_t& size);

private:
	FLevel& Level;
	TArray<WORD> BlockMap;

	void BuildBlockmap();
	void CreateUnpackedBlockmap(TArray<WORD>* blocks, int bmapwidth, int bmapheight);
	void CreatePackedBlockmap(TArray<WORD>* blocks, int bmapwidth, int bmapheight);
};
