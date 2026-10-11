"""Generate/verify only synthetic editor fixtures; never unpickle game saves."""
import argparse
import base64
import json
import pathlib
import pickle
import sys
import types

renpy = types.ModuleType("renpy")
revertable = types.ModuleType("renpy.revertable")
renpy.revertable = revertable
sys.modules.update({"renpy": renpy, "renpy.revertable": revertable})


class RevertableList(list):
    pass


class RevertableDict(dict):
    pass


class RevertableSet(set):
    # Matches Ren'Py's serialized state contract, without importing game code.
    def __getstate__(self):
        return ({member: True for member in sorted(self)},)

    def __setstate__(self, state):
        self.update(state[0].keys())

    __reduce__ = object.__reduce__
    __reduce_ex__ = object.__reduce_ex__


for cls in (RevertableList, RevertableDict, RevertableSet):
    cls.__module__ = "renpy.revertable"
    setattr(revertable, cls.__name__, cls)


class Persistent:
    pass


class Custom:
    pass


def sample():
    data = Persistent()
    data.data = {"list": [5, False, None], "tuple": ("text", 2.5), "dict": {"flag": False}}
    data.wrapper = RevertableDict(flags=RevertableList([False, 12]))
    data.empty_list = RevertableList()
    data.gallery_unlocked = RevertableSet(["cg_a", "cg_b"])
    data.native_set = {"keep"}
    data.frozen = frozenset(["keep"])
    data.null_value = None
    shared = [{"score": 9}]
    data.shared_a = shared
    data.shared_b = shared
    cycle = []
    cycle.append(cycle)
    data.cycle = cycle
    data.custom = Custom()
    data.custom.keep = [1, 2]
    data.large = 1 << 100
    data._changed = {key: 123.0 for key in vars(data)}
    return data


def verify(directory):
    # These are exports of the synthetic sample above, NOT external/game pickles.
    for protocol in range(6):
        data = pickle.loads((directory / f"collections-{protocol}.pickle").read_bytes())
        assert data.data == {"list": [42, False, "initialized"], "tuple": ("text", 3.75), "dict": {"flag": False}}
        assert isinstance(data.data["list"], list)
        assert isinstance(data.data["tuple"], tuple)
        assert data.null_value is True
        assert data.shared_a is data.shared_b and data.shared_a == [{"score": 9}]
        assert data.cycle[0] is data.cycle
        assert data.custom.keep == [1, 2] and data.large == 1 << 100
        assert data._changed["data"] > 123.0 and data._changed["null_value"] > 123.0
        if protocol >= 2:
            assert type(data.gallery_unlocked) is RevertableSet
            assert data.gallery_unlocked == {"cg_b", "cg_new"}
            assert data.wrapper["flags"] == [True, 12]
            assert isinstance(data.wrapper, RevertableDict)
            assert isinstance(data.wrapper["flags"], RevertableList)
        else:
            assert data.gallery_unlocked == {"cg_a", "cg_b"}
    print("Independent Python roundtrip: 6 protocols passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=pathlib.Path)
    args = parser.parse_args()
    if args.verify:
        verify(args.verify)
    else:
        target = pathlib.Path(__file__).resolve().parents[1] / "src-tauri/src/save_editor/fixtures/renpy-collections.json"
        fixtures = [{"protocol": p, "persistent": base64.b64encode(pickle.dumps(sample(), protocol=p)).decode("ascii")} for p in range(6)]
        target.write_text(json.dumps(fixtures, indent=2) + "\n", encoding="utf-8")
