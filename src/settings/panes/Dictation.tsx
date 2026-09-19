import { useEffect, useState } from "react";
import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Group, Note, Pane, Row, Select } from "../ui";

export default function DictationPane(_: PaneProps) {
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");

  useEffect(() => {
    void api.getMicrophones().then(setMicrophones);
    void api.getSettingStr("selected_microphone").then(setSelectedMic);
  }, []);

  return (
    <Pane title="Dictation" description="Speech to text, entirely on this machine.">
      <Group title="Input">
        <Row label="Microphone" hint="The system default is used unless you pick one.">
          <Select
            label="Microphone"
            value={selectedMic}
            onChange={(name) => {
              setSelectedMic(name);
              void api.setMicrophone(name);
            }}
            options={[
              { value: "", label: "System default" },
              ...microphones.map((m) => ({ value: m, label: m })),
            ]}
          />
        </Row>
      </Group>

      <Note>
        Voice activity detection, filler-word removal, paste behaviour, language and
        post-processing are not wired to settings yet — see docs/PARITY.md, section B.
      </Note>
    </Pane>
  );
}
