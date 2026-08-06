export const MyTruncate = ({
  value,
  maxWidth = "120px",
}: {
  value: string;
  maxWidth?: string;
}) => {
  return (
    <div
      className="truncate whitespace-nowrap overflow-hidden"
      style={{ maxWidth }}
    >
      {value}
    </div>
  );
};
