import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { failure } from "./errors";

type Props = {
	root: string;
	/** The name of the project's copy, or nothing if there is no cover yet. */
	cover: string;
	onChange: (cover: string) => void;
};

/**
 * The project's cover image as something an `img` can show. The bytes come back
 * through a command and become an object URL. Pointing an `img` straight at the
 * file would mean turning on the asset protocol with a path scope wide enough
 * to reach wherever projects are kept.
 *
 * `stamp` reloads the image when the file was replaced under the same name.
 */
export function useCoverImage(root: string, cover: string, stamp: number) {
	const [src, setSrc] = useState("");
	const [trouble, setTrouble] = useState("");

	useEffect(() => {
		if (cover === "") {
			setSrc("");
			setTrouble("");
			return;
		}

		let gone = false;
		let url = "";

		invoke<ArrayBuffer>("read_cover", { root })
			.then((bytes) => {
				if (!gone) {
					url = URL.createObjectURL(new Blob([bytes]));
					setSrc(url);
					setTrouble("");
				}
			})
			.catch((error: unknown) => {
				if (!gone) {
					setSrc("");
					setTrouble(failure(error).message);
				}
			});

		return () => {
			gone = true;
			if (url !== "") {
				URL.revokeObjectURL(url);
			}
		};
	}, [root, cover, stamp]);

	return { src, trouble };
}

/**
 * The book's cover. What the writer picks is copied into the project, so the
 * book names a file beside `aurora.json` rather than a path into their home
 * directory: the project folder carries its own cover wherever it goes, and
 * moving or deleting the original breaks nothing.
 */
export default function Cover({ root, cover, onChange }: Props) {
	// Bumped when a cover is chosen, so that replacing a PNG with another PNG
	// reloads the image even though the name it is filed under has not changed.
	const [stamp, setStamp] = useState(0);
	const { src, trouble: unread } = useCoverImage(root, cover, stamp);
	const [refused, setRefused] = useState("");
	const trouble = refused === "" ? unread : refused;

	async function choose() {
		const picked = await open({
			multiple: false,
			title: "Choose a cover image",
			filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }],
		});

		if (typeof picked !== "string") {
			return;
		}

		try {
			const name = await invoke<string>("set_cover", {
				root,
				source: picked,
			});
			setStamp((n) => n + 1);
			setRefused("");
			onChange(name);
		} catch (error) {
			setRefused(failure(error).message);
		}
	}

	async function remove() {
		try {
			await invoke("clear_cover", { root });
			setRefused("");
			onChange("");
		} catch (error) {
			setRefused(failure(error).message);
		}
	}

	return (
		<div className="cover">
			{/* The frame keeps its size whether it is holding the image or a
			    complaint about it, so nothing below moves. */}
			<div className="cover__frame">
				{src === "" ? (
					<p className="cover__empty">
						{cover === ""
							? "No cover yet"
							: trouble === ""
								? "Loading"
								: trouble}
					</p>
				) : (
					<img className="cover__image" src={src} alt="" />
				)}
			</div>

			<div className="cover__buttons">
				<button
					type="button"
					className="cover__button"
					onClick={() => void choose()}
				>
					{cover === "" ? "Choose an image" : "Choose another"}
				</button>
				{cover !== "" && (
					<button
						type="button"
						className="cover__button"
						onClick={() => void remove()}
					>
						Remove
					</button>
				)}
			</div>

			<p className="cover__note">
				PNG or JPEG. Aurora keeps its own copy in the project folder.
			</p>
		</div>
	);
}
