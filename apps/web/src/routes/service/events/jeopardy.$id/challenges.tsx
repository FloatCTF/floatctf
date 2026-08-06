import {
  CheckIcon,
  SparkleFillIcon,
  SparkleIcon,
  SparklesFillIcon,
  TriangleDownIcon,
} from "@primer/octicons-react";
import { Button, Label, SelectPanel, Spinner, TextInput } from "@primer/react";

import type { UniResponse } from "@/api/axios";
import { Banner, DataTable, Dialog, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useReactive } from "ahooks";
import { useCallback, useEffect, useMemo, useState } from "react";
import { RemainingTimer } from "../../challenges/$id";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import type { Challenges, Instances } from "@/entity";
import type { AxiosError } from "axios";
export const Route = createFileRoute("/service/events/jeopardy/$id/challenges")(
  {
    component: RouteComponent,
  }
);

export type EventChallengeResult = {
  id: string;
  challenge: Challenges;
  current_points: number;
  solved_count: number;
  solved: boolean;
  solved_no: number;
};

function RouteComponent() {
  const { id } = Route.useParams();
  const { data, isLoading, isError, error } = useQuery<
    UniResponse<EventChallengeResult[]>,
    AxiosError<{ message: string }>
  >({
    queryKey: ["eventChallenges", id],
    queryFn: () => serviceApi.events.fetchChallenges(id),
  });

  const challengeDialog = useReactive({
    open: false,
    title: "",
    event_challenge_result: {} as EventChallengeResult,
  });

  const handleClose = useCallback(() => {
    challengeDialog.open = false;
  }, [challengeDialog]);
  const categories = [
    { text: "ALL" },
    { text: "Web" },
    { text: "Misc" },
    { text: "Pwn" },
    { text: "Crypto" },
    { text: "Reverse" },
    { text: "AI" },
  ];
  const [selected, setSelected] = useState(categories[0]); // 默认 ALL
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState(false);
  const filteredData = useMemo(() => {
    if (!data?.data) return [];
    if (selected.text === "ALL") return data.data;
    return data.data.filter(
      (row: EventChallengeResult) =>
        row.challenge.category.toLowerCase() === selected.text.toLowerCase()
    );
  }, [data?.data, selected]);
  // TODO: add filter to GenericTable
  const filteredItems = categories.filter(
    (item) =>
      item.text === selected?.text || // 保证选中的值始终显示
      item.text.toLowerCase().includes(filter.toLowerCase())
  );
  const columns = [
    {
      accessorKey: "challenge.name",
      header: "Name",
      field: "challenge.name",
      rowHeader: true,
      renderCell: (row: EventChallengeResult) => {
        return (
          <button
            type="button"
            onClick={() => {
              challengeDialog.title = row.challenge.name;
              challengeDialog.event_challenge_result = row;
              challengeDialog.open = true;
            }}
            className="bg-transparent border-none p-0 m-0 text-blue-600 hover:underline cursor-pointer"
          >
            {row.challenge.name}
          </button>
        );
      },
    },

    {
      accessorKey: "current_points",
      header: "Points",
      field: "current_points",
      renderCell: (row: EventChallengeResult) => {
        return (
          <span className="font-bold">{row.current_points.toFixed(2)}</span>
        );
      },
      sortBy: true,
    },
    {
      accessorKey: "challenge.category",
      header: () => {
        return (
          <SelectPanel
            renderAnchor={({ children, ...anchorProps }) => (
              <Button
                {...anchorProps}
                trailingAction={TriangleDownIcon}
                aria-haspopup="dialog"
              >
                {selected?.text ?? "Category"}
              </Button>
            )}
            placeholder="Pick category"
            open={open}
            onOpenChange={setOpen}
            items={filteredItems}
            selected={selected}
            // @ts-ignore
            onSelectedChange={setSelected}
            onFilterChange={setFilter}
          />
        );
      },
      field: "challenge.category",
    },
    {
      accessorKey: "solved_count",
      header: "Solved Count",
      field: "solved_count",
      sortBy: true,
    },
    {
      accessorKey: "solved",
      header: "Solved",
      field: "solved",
      renderCell: (row: EventChallengeResult) => {
        if (row.solved) {
          if (row.solved_no === 1) return <SparklesFillIcon size={16} />;
          if (row.solved_no === 2) return <SparkleFillIcon size={16} />;
          if (row.solved_no === 3) return <SparkleIcon size={16} />;
        }
        return row.solved ? <CheckIcon size={16} /> : <></>;
      },
      sortBy: true,
    },
  ];
  const table = useReactTable({
    data: filteredData,
    columns: columns,
    getCoreRowModel: getCoreRowModel(),
  });

  if (isLoading) {
    return <Spinner size="large" />;
  }
  if (isError) {
    const msg =
      error.response?.data?.message || error.message || "Unknown error";
    return <div>{msg}</div>;
  }

  return (
    <Table.Container className="m-2">
      <DataTable
        aria-labelledby="repositories-default"
        // @ts-ignore
        columns={columns}
        data={table.getRowModel().rows.map((row) => row.original)}
      />
      <ChallengeDialog
        open={challengeDialog.open}
        title={challengeDialog.title}
        event_challenge_result={challengeDialog.event_challenge_result}
        onClose={handleClose}
        eventId={id}
      />
    </Table.Container>
  );
}
type ChallengeDialogProps = {
  open: boolean;
  title: string;
  event_challenge_result: EventChallengeResult | null;
  onClose: () => void;
  eventId: string;
};

function ChallengeDialog({
  open,
  title,
  event_challenge_result,
  onClose,
  eventId,
}: ChallengeDialogProps) {
  const challenge = event_challenge_result?.challenge;
  const queryClient = useQueryClient();

  const challengeStatus = useReactive({
    isRunning: false,
    instance: {} as Instances,
    flag: "",
  });
  // 拉取现有 instance
  const { data: instance_data, refetch: refetch_instance } = useQuery({
    queryKey: ["event_instance", challenge?.id],
    queryFn: () =>
      serviceApi.events.getChallengeInstance(eventId, challenge?.id ?? ""),
    enabled: open && !!challenge?.id,
  });

  // useEffect 只在 instance_data 更新时执行一次
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    if (open) {
      if (instance_data?.data) {
        challengeStatus.isRunning = true;
        challengeStatus.instance = instance_data.data;
      } else {
        challengeStatus.isRunning = false;
        challengeStatus.instance = {} as Instances;
      }
    }
  }, [instance_data, open]);
  const banner = useMsgBanner();
  // Launch mutation
  const launchMutation = useMutation({
    mutationFn: (challenge_id: string) =>
      serviceApi.events.launchSingleInstance(eventId, challenge_id),
    onSuccess: (data) => {
      challengeStatus.isRunning = true;
      challengeStatus.instance = data.data!;
    },
    onError: (error) => {
      banner.showErrorBanner(error);
    },
  });
  const destroyInstance = useMutation({
    mutationFn: serviceApi.instances.destroy,
    onSuccess: (_data) => {
      challengeStatus.isRunning = false;
      challengeStatus.instance = {} as Instances;
    },
    onError: (error) => {
      banner.showErrorBanner(error);
    },
  });
  const submitFlag = useMutation({
    mutationFn: serviceApi.submit.submitSingle,
    onSuccess: (_data) => {
      banner.showBanner("success", "Flag is correct!");
      queryClient.invalidateQueries({
        queryKey: ["eventChallenges", eventId],
      });
      // close in the backend
      challengeStatus.isRunning = false;
      challengeStatus.instance = {} as Instances;
    },
    onError: (error) => {
      banner.showErrorBanner(error);
    },
  });

  if (!open) return null;

  return (
    <Dialog
      title={challenge?.name ?? title}
      onClose={() => {
        banner.hideBanner();
        onClose();
      }}
      className="max-w-lg"
    >
      <div className="flex flex-col gap-2 text-sm text-gray-800">
        {/* 状态信息 */}
        <div className="flex flex-wrap gap-4">
          <div>
            <span className="font-semibold">Current Score:</span>{" "}
            {event_challenge_result?.current_points.toFixed(2) ?? "-"}
          </div>
          <div>
            <span className="font-semibold">Solved Count:</span>{" "}
            {event_challenge_result?.solved_count ?? "-"}
          </div>
          <div>
            <span className="font-semibold">Solved:</span>{" "}
            {event_challenge_result?.solved ? "✅" : "❌"}
          </div>
        </div>

        {/* 题目信息 */}
        <p>{challenge?.description}</p>

        {/* 附件 */}
        {challenge?.attachment && (
          <a
            href={`/challenges/${challenge.safe_name}/${challenge.attachment}`}
            download
            target="_blank"
            rel="noopener noreferrer"
            className="inline-block mt-1"
          >
            <Label variant="accent">{challenge.attachment}</Label>
          </a>
        )}

        {/* Instance 控制 */}
        <div className="mt-2 ">
          <banner.BannerComponent />
          {challengeStatus.instance && (
            <div
              // biome-ignore lint/security/noDangerouslySetInnerHtml: <explanation>
              dangerouslySetInnerHTML={{
                __html: challengeStatus.instance.content || "",
              }}
            />
          )}
          {challengeStatus.isRunning ? (
            <div className="w-full flex flex-col gap-2 mb-4">
              <RemainingTimer
                destroy_at={challengeStatus.instance.destroy_at}
                onExpire={() => {
                  refetch_instance({ cancelRefetch: true });
                  destroyInstance.mutate(challengeStatus.instance.id);
                }}
              />

              <div className="flex gap-2">
                <TextInput
                  className="flex-1"
                  value={challengeStatus.flag}
                  onChange={(e) => {
                    challengeStatus.flag = e.target.value;
                  }}
                  placeholder="flag{}"
                />
                <Button
                  variant="danger"
                  onClick={() => {
                    destroyInstance.mutate(challengeStatus.instance.id);
                  }}
                >
                  Destroy
                </Button>

                <Button
                  variant="primary"
                  onClick={() => {
                    submitFlag.mutate({
                      event_id: eventId,
                      instance_id: challengeStatus.instance.id,
                      flag: challengeStatus.flag,
                    });
                  }}
                >
                  Submit
                </Button>
              </div>
            </div>
          ) : (
            <Button
              variant="primary"
              onClick={() => {
                challengeStatus.flag = "";
                launchMutation.mutate(challenge?.id ?? "");
              }}
            >
              Launch
            </Button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
