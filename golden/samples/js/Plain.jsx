export default function Plain(props) {
  if (props.hidden) {
    return null;
  }
  return <div title="plain">{props.children}</div>;
}
