import { Tag } from "@/types";
import { invoke } from "@tauri-apps/api/core";

export async function fetchTags(): Promise<Tag[]> {
  try {
    const tags: Tag[] = await invoke("get_tags");
    return tags;
  } catch (error) {
    throw new Error(`Failed to fetch tags: ${error}`);
  }
}

export async function createTag(
  name: string,
  color: string = "#3489eb"
): Promise<Tag> {
  try {
    return await invoke("create_tag", { name, color });
  } catch (error) {
    throw new Error(`Failed to create tag: ${error}`);
  }
}
