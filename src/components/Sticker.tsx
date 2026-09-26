import { useEffect, useState } from "react";
import { api } from "../api";

// Miniaturas já carregadas (object URLs), por conversa + arquivo. Promessas em voo são
// compartilhadas para que várias figurinhas iguais na tela gerem uma chamada só.
const cache = new Map<string, Promise<string | null>>();

function load(id: string, file: string) {
  const key = `${id}\u0000${file}`;
  let p = cache.get(key);
  if (!p) {
    p = api
      .stickerThumb(id, file)
      .then((buf) => (buf.byteLength ? URL.createObjectURL(new Blob([buf], { type: "image/png" })) : null))
      .catch(() => null);
    cache.set(key, p);
  }
  return p;
}

export function Sticker({ id, file, size = 56, label }: { id: string; file: string; size?: number; label: string }) {
  const [url, setUrl] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let alive = true;
    load(id, file).then((u) => alive && setUrl(u));
    return () => {
      alive = false;
    };
  }, [id, file]);
  if (url === null) return <div className="sticker missing" style={{ width: size, height: size }} title="Arquivo indisponível">?</div>;
  return (
    <div className="sticker" style={{ width: size, height: size }}>
      {url && <img src={url} alt={label} width={size} height={size} />}
    </div>
  );
}
