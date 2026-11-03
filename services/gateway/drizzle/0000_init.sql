CREATE TABLE IF NOT EXISTS "users" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text NOT NULL,
	"provider_sub" text NOT NULL,
	"email" text,
	"display_name" text,
	"monthly_quota_minutes" integer DEFAULT 1200 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE IF NOT EXISTS "usage" (
	"user_id" text NOT NULL,
	"year_month" text NOT NULL,
	"live_minutes" double precision DEFAULT 0 NOT NULL,
	"final_minutes" double precision DEFAULT 0 NOT NULL,
	"llm_tokens" bigint DEFAULT 0 NOT NULL,
	CONSTRAINT "usage_user_id_year_month_pk" PRIMARY KEY("user_id","year_month")
);
--> statement-breakpoint
DO $$ BEGIN
	ALTER TABLE "usage" ADD CONSTRAINT "usage_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "users"("id") ON DELETE cascade ON UPDATE no action;
EXCEPTION
	WHEN duplicate_object THEN null;
END $$;
--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS "users_provider_sub_idx" ON "users" ("provider","provider_sub");
