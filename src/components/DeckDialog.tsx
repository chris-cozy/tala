import { useState } from "react";
import { ImagePlus, X, Layers } from "lucide-react";
import type { Deck } from "../bindings/Deck";
import { useTala, Field } from "../lib/context";
import { mediaUrl, pickFile, rpc } from "../lib/api";
import { Button, Modal } from "./ui";
import { branchIds } from "../lib/decks";
export const deckColors = ["violet", "teal", "blue", "rose", "amber", "slate"];
export function DeckArt({
  deck,
  mediaDir,
}: {
  deck: Pick<Deck, "cover" | "color">;
  mediaDir: string;
}) {
  return (
    <div className={`deck-art color-${deck.color}`}>
      {deck.cover ? (
        <img
          src={mediaUrl(mediaDir, deck.cover)}
          alt=""
          onError={(e) => {
            e.currentTarget.style.opacity = "0";
          }}
        />
      ) : (
        <>
          <div className="art-orbit orbit-one" />
          <div className="art-orbit orbit-two" />
          <div className="art-orbit orbit-three" />
          <div className="art-star">✦</div>
        </>
      )}
      <div className="art-shade" />
    </div>
  );
}
export default function DeckDialog({
  deck,
  parentId: initialParentId,
  onClose,
}: {
  deck?: Deck;
  parentId?: string;
  onClose: () => void;
}) {
  const { data, run, navigate } = useTala();
  const [parentId, setParentId] = useState(
    deck?.parentId ?? initialParentId ?? "",
  );
  const excluded = deck ? branchIds(data.decks, deck.id) : new Set<string>();
  const [name, setName] = useState(deck?.name ?? "");
  const [color, setColor] = useState(deck?.color ?? "violet");
  const [cover, setCover] = useState<string | null>(deck?.cover ?? null);
  const [busy, setBusy] = useState(false);
  async function attach() {
    const result = await run(async () => {
      const file = await pickFile("image");
      return file
        ? rpc({ action: "attach_file", payload: { token: file.token } })
        : null;
    });
    if (result) setCover(result);
  }
  async function save() {
    setBusy(true);
    const id = await run(
      () =>
        rpc({
          action: "save_deck",
          payload: {
            id: deck?.id ?? null,
            name,
            parentId: parentId || null,
            color,
            cover,
            settings: deck?.settings ?? data.preferences.defaults,
          },
        }),
      deck ? "Deck updated" : "Deck created",
    );
    setBusy(false);
    if (id) {
      onClose();
      if (!deck) await navigate({ page: "deck", id });
    }
  }
  return (
    <Modal
      title={deck ? "Edit deck" : "A new place to learn"}
      description={
        deck
          ? "Make this deck your own."
          : "Give your next collection of ideas a home."
      }
      open
      onClose={onClose}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <div className="deck-dialog-grid">
          <div className="cover-preview">
            <DeckArt deck={{ color, cover }} mediaDir={data.mediaDir} />
            <Layers size={28} />
          </div>
          <div className="stack">
            <Field label="Deck name">
              <input
                autoFocus
                placeholder="e.g. Biology, Japanese, big ideas…"
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={200}
              />
            </Field>
            <Field label="Parent deck">
              <select
                value={parentId}
                onChange={(e) => setParentId(e.target.value)}
              >
                <option value="">None · top-level deck</option>
                {data.decks
                  .filter((candidate) => !excluded.has(candidate.id))
                  .map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {candidate.path}
                    </option>
                  ))}
              </select>
            </Field>
            <Field label="Color">
              <div className="color-options">
                {deckColors.map((c) => (
                  <button
                    type="button"
                    key={c}
                    className={`color-dot color-${c} ${c === color ? "selected" : ""}`}
                    aria-label={`${c} deck color`}
                    aria-pressed={c === color}
                    onClick={() => setColor(c)}
                  />
                ))}
              </div>
            </Field>
            <div className="inline">
              <Button onClick={attach}>
                <ImagePlus size={16} />
                {cover ? "Replace cover" : "Add cover"}
              </Button>
              {cover && (
                <Button
                  variant="ghost"
                  onClick={() => setCover(null)}
                  aria-label="Remove cover"
                >
                  <X size={16} />
                </Button>
              )}
            </div>
            <small className="muted">
              PNG, JPEG, WebP, or GIF · up to 20 MB
            </small>
          </div>
        </div>
        <div className="modal-actions">
          <Button onClick={onClose}>Cancel</Button>
          <Button
            type="submit"
            variant="primary"
            busy={busy}
            disabled={!name.trim()}
          >
            {deck ? "Save changes" : "Create deck"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
