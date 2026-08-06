import { adminApi } from "@/api";
import { Button, FormControl, Stack, Textarea } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useState } from "react";

export const Route = createFileRoute("/admin/database")({
  component: RouteComponent,
});

export type SqlStatement = {
  sql: string;
};

export type SqlResult = {
  sql_type: string;
  // biome-ignore lint/suspicious/noExplicitAny: <explanation>
  rows: Record<string, any>[];
  count: number;
  rows_affected: number;
  elapsed_ms: number;
};

function RouteComponent() {
  const sql_state = useReactive({
    show: false,
    sql_msg: "",
    sql_msg_variant: "error" as "error" | "success",
    sql: "",
    data: {} as SqlResult,
  });

  const exec_sql = useMutation({
    mutationFn: adminApi.database.exec_sql,
    onSuccess: (data) => {
      if (data.data) {
        sql_state.data = data.data;
        console.log(sql_state.data.rows);
        sql_state.sql_msg = `Executed successfully, ${sql_state.data.count} rows • ${sql_state.data.elapsed_ms} ms`;
        sql_state.sql_msg_variant = "success";
        sql_state.show = true;
      }
    },
    onError: (error) => {
      sql_state.data = {} as SqlResult;
      sql_state.sql_msg_variant = "error";
      sql_state.sql_msg =
        (error as AxiosError<{ message: string }>)?.response?.data?.message ||
        (error as Error).message ||
        "Unknown error";
      sql_state.show = true;
    },
  });

  return (
    <div className="w-full h-full">
      <FormControl>
        <FormControl.Label>SQL Execution (PostgreSQL)</FormControl.Label>
        <Textarea
          className="w-full"
          resize="none"
          value={sql_state.sql}
          onChange={(e) => {
            sql_state.sql = e.target.value;
          }}
        />
        {sql_state.show && (
          <FormControl.Validation variant={sql_state.sql_msg_variant}>
            {sql_state.sql_msg}
          </FormControl.Validation>
        )}
        {/* Operation */}
        <Stack direction="horizontal">
          <Button
            onClick={() => {
              sql_state.data = {} as SqlResult;
              exec_sql.mutate({ sql: sql_state.sql });
            }}
          >
            Run SQL
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              sql_state.data = {} as SqlResult;
              sql_state.show = false;
            }}
          >
            Clear
          </Button>
        </Stack>
        <SqlDataTable result={sql_state.data} />
      </FormControl>
    </div>
  );
}

function SqlDataTable({ result }: { result: SqlResult }) {
  const rawRows = result.rows ?? [];
  const rows = rawRows.map((r, i) => ({ ...r }));

  const columns =
    rows.length > 0
      ? Object.keys(rows[0]).map((key) => ({
          field: key,
          header: key,
        }))
      : [];

  // 没数据时直接返回
  if (rows.length === 0) {
    return (
      <Stack>
        <code>
          查看所有数据库 SELECT datname FROM pg_database WHERE datistemplate =
          false;
        </code>

        <code>
          查看所有表 SELECT tablename FROM pg_tables WHERE schemaname NOT IN
          ('pg_catalog', 'information_schema');
        </code>
        <code>
          查看当前 SELECT tablename FROM pg_tables WHERE schemaname = 'schema';
          可指定 schema
        </code>
        <code>
          查看表结构 SELECT column_name, data_type, is_nullable, column_default
          FROM information_schema.columns WHERE table_name = 'users';
        </code>
        <code>查看建表语句 SELECT pg_get_tabledef('users'::regclass);</code>
        <code>查看所有列定义 同上 (information_schema.columns)</code>
        <code>查看当前数据库名 SELECT current_database();</code>
        <code>查看当前用户 SELECT current_user;</code>
        <code>查看服务器版本 SHOW server_version;</code>
      </Stack>
    );
  }

  return (
    <Table.Container>
      <Table.Title id="sql_results">Results</Table.Title>
      <DataTable
        aria-labelledby="repositories-default"
        // @ts-ignore
        data={rows}
        // @ts-ignore
        columns={columns}
        cellPadding="condensed"
      />
    </Table.Container>
  );
}
