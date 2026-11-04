import React, { useState, useEffect } from 'react';
import { format } from './helpers';

export function Badge({ label }) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(format(label).length);
  }, [label]);
  const bump = () => setCount(count + 1);
  return <span className="badge" onClick={bump}>{label}: {count}</span>;
}
