#ifndef __DOOMDATA_H__
#define __DOOMDATA_H__

#ifdef _MSC_VER
#pragma once
#endif

#include "tarray.hpp"
#include "common.hpp"

#include "zdbsp.h"

enum {
	BOXTOP,
	BOXBOTTOM,
	BOXLEFT,
	BOXRIGHT
};

struct UDMFKey {
	const char* key;
	const char* value;
};

struct MapVertex {
	short x, y;
};

struct WideVertex {
	fixed_t x, y;
	int index;
};

struct MapSideDef {
	short textureoffset;
	short rowoffset;
	char toptexture[8];
	char bottomtexture[8];
	char midtexture[8];
	WORD sector;
};

struct IntSideDef {
	// the first 5 values are only used for binary format maps
	short textureoffset;
	short rowoffset;
	char toptexture[8];
	char bottomtexture[8];
	char midtexture[8];

	int sector;

	TArray<UDMFKey> props;
};

struct MapLineDef {
	WORD v1;
	WORD v2;
	short flags;
	short special;
	short tag;
	WORD sidenum[2];
};

struct MapLineDef2 {
	WORD v1;
	WORD v2;
	short flags;
	unsigned char special;
	unsigned char args[5];
	WORD sidenum[2];
};

struct IntLineDef {
	DWORD v1;
	DWORD v2;
	int flags;
	int special;
	int args[5];
	DWORD sidenum[2];

	TArray<UDMFKey> props;
};

struct MapSector {
	short floorheight;
	short ceilingheight;
	char floorpic[8];
	char ceilingpic[8];
	short lightlevel;
	short special;
	short tag;
};

struct IntSector {
	// none of the sector properties are used by the node builder
	// so there's no need to store them in their expanded form for
	// UDMF. Just storing the UDMF keys and leaving the binary fields
	// empty is enough
	MapSector data;

	TArray<UDMFKey> props;
};

struct MapSubsector {
	WORD numlines;
	WORD firstline;
};

struct MapSubsectorEx {
	DWORD numlines;
	DWORD firstline;
};

struct MapSeg {
	WORD v1;
	WORD v2;
	WORD angle;
	WORD linedef;
	short side;
	short offset;
};

struct MapSegEx {
	DWORD v1;
	DWORD v2;
	WORD angle;
	WORD linedef;
	short side;
	short offset;
};

struct MapSegGL {
	WORD v1;
	WORD v2;
	WORD linedef;
	WORD side;
	WORD partner;
};

struct MapSegGLEx {
	DWORD v1;
	DWORD v2;
	DWORD linedef;
	WORD side;
	DWORD partner;
};

#define NF_SUBSECTOR 0x8000
#define NFX_SUBSECTOR 0x80000000

struct MapNodeExO {
	short x, y, dx, dy;
	short bbox[2][4];
	DWORD children[2];
};

struct MapThing {
	short x;
	short y;
	short angle;
	short type;
	short flags;
};

struct MapThing2 {
	unsigned short thingid;
	short x;
	short y;
	short z;
	short angle;
	short type;
	short flags;
	char special;
	char args[5];
};

struct IntThing {
	unsigned short thingid;
	fixed_t x; // full precision coordinates for UDMF support
	fixed_t y;
	// everything else is not needed or has no extended form in UDMF
	short z;
	short angle;
	short type;
	short flags;
	char special;
	char args[5];

	TArray<UDMFKey> props;
};

struct IntVertex {
	TArray<UDMFKey> props;
};

struct FLevel {
	FLevel();
	~FLevel();

	WideVertex* Vertices;
	size_t NumVertices;
	TArray<IntVertex> VertexProps;
	TArray<IntSideDef> Sides;
	TArray<IntLineDef> Lines;
	TArray<IntSector> Sectors;
	TArray<IntThing> Things;
	MapSubsectorEx* Subsectors;
	size_t NumSubsectors;
	MapSegEx* Segs;
	size_t NumSegs;
	zdbsp_MapNodeEx* Nodes;
	size_t NumNodes;
	WORD* Blockmap;
	size_t BlockmapSize;
	BYTE* Reject;
	size_t RejectSize;

	MapSubsectorEx* GLSubsectors;
	size_t NumGLSubsectors;
	MapSegGLEx* GLSegs;
	size_t NumGLSegs;
	zdbsp_MapNodeEx* GLNodes;
	size_t NumGLNodes;
	WideVertex* GLVertices;
	size_t NumGLVertices;
	BYTE* GLPVS;
	size_t GLPVSSize;

	int NumOrgVerts;

	DWORD* OrgSectorMap;
	int NumOrgSectors;

	fixed_t MinX, MinY, MaxX, MaxY;

	TArray<UDMFKey> props;

	void FindMapBounds();
	void RemoveExtraLines();
	void RemoveExtraSides();
	void RemoveExtraSectors();

	uint32_t NumSides() const {
		return Sides.Size();
	}
	uint32_t NumLines() const {
		return Lines.Size();
	}
	uint32_t NumSectors() const {
		return Sectors.Size();
	}
	uint32_t NumThings() const {
		return Things.Size();
	}
};

const int BLOCKSIZE = 128;
const int BLOCKFRACSIZE = BLOCKSIZE << FRACBITS;
const int BLOCKBITS = 7;
const int BLOCKFRACBITS = FRACBITS + 7;

#endif //__DOOMDATA_H__
