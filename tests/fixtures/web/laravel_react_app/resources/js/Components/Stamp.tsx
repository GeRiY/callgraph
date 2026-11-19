interface StampProps {
    label: string;
}

export default function Stamp({ label }: StampProps) {
    return <span className="stamp">{label}</span>;
}
