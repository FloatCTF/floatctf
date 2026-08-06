-- for updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
   NEW."updated_at" = now();
   RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$
DECLARE
    t text;
    trigger_name text;
BEGIN
  FOR t IN
      SELECT table_name
      FROM information_schema.columns
      WHERE table_schema = 'public'
        AND column_name = 'updated_at'
  LOOP
      trigger_name := format('trg_%s_updated_at', t);

      -- 如果触发器存在，则删除
      IF EXISTS (
          SELECT 1
          FROM pg_trigger
          WHERE tgname = trigger_name
            AND tgrelid = (SELECT oid FROM pg_class WHERE relname = t)
      ) THEN
          EXECUTE format('DROP TRIGGER %I ON %I;', trigger_name, t);
      END IF;

      -- 创建新的触发器
      EXECUTE format(
          'CREATE TRIGGER trg_%I_updated_at
             BEFORE UPDATE ON %I
             FOR EACH ROW
             EXECUTE FUNCTION update_updated_at_column();',
          t, t
      );
  END LOOP;
END$$;