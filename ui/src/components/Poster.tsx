import { useState } from "react";
import { posterSrc, type Anime } from "../api";

/** Cover image, with the title's initial while loading or if it fails. */
export function Poster({
  anime,
  title,
  className = "poster",
}: {
  anime: Anime | null | undefined;
  title: string;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const url = anime?.imageUrl;
  return (
    <div className={className} aria-hidden="true">
      <span className="poster-fallback">{title.charAt(0).toUpperCase()}</span>
      {url && !failed && (
        <img src={posterSrc(url)} alt="" loading="lazy" draggable={false} onError={() => setFailed(true)} />
      )}
    </div>
  );
}
