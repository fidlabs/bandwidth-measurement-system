-- create table from dmob db: unified_verified_deal
-- required for sqlx prepare to work...
CREATE TABLE IF NOT EXISTS public.unified_verified_deal (
    "id" SERIAL NOT NULL PRIMARY KEY,
    "dealId" integer NOT NULL DEFAULT 0,
    "claimId" integer NOT NULL DEFAULT 0,
    "type" character varying,
    "clientId" character varying,
    "providerId" character varying,
    "sectorId" character varying,
    "pieceCid" character varying,
    "pieceSize" numeric,
    "termMax" numeric,
    "termMin" numeric,
    "termStart" numeric,
    "slashedEpoch" numeric NOT NULL DEFAULT '0',
    "processedSlashedEpoch" integer NOT NULL DEFAULT 0,
    "removed" boolean DEFAULT false,
    "createdAt" timestamp without time zone NOT NULL DEFAULT now(),
    "updatedAt" timestamp without time zone NOT NULL DEFAULT now()
);

CREATE INDEX unified_verified_deal_claimid_index ON public.unified_verified_deal ("claimId");
CREATE INDEX unified_verified_deal_clientid_index ON public.unified_verified_deal ("clientId");
CREATE INDEX unified_verified_deal_dealid_index ON public.unified_verified_deal ("dealId");
CREATE INDEX unified_verified_deal_piececid_index ON public.unified_verified_deal ("pieceCid");
CREATE INDEX unified_verified_deal_providerid_index ON public.unified_verified_deal ("providerId");
CREATE INDEX unified_verified_deal_sectorid_index ON public.unified_verified_deal ("sectorId");