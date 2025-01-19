import json

def repeat(beats, size=4, times=2):
    """
    Utility: replicate a set of beats every 'size' quarter notes, 'times' times.
    e.g. repeat([0, 0.75], size=2, times=4) -> [0, 0.75, 2, 2.75, 4, 4.75, 6, 6.75]
    """
    repeated_beats = list(beats)
    for i in range(1, times):
        offset = size * i
        for b in beats:
            repeated_beats.append(b + offset)
    return repeated_beats

def generate_patterns():
    patterns = [
        {
            "sound": "bd",
            # "beats": repeat([0.0, 0.75, 2.0, 2.75, 3.25], 4, 4),
            "beats": repeat([0.0, 2.0, 2.75, 3.25], 4, 4),
            # "beats": repeat([0, 1], 2, 8),
            "velocity": 70.0,
            "duration": 0.25,
        },
        {
            "sound": "claps",
            "beats": repeat([1.5], 4, 4),
            # "beats": repeat([1], 4, 4),
            "velocity": 50.0,
            "duration": 0.25,
        },
        {
            "sound": "sd",
            "beats": repeat([3.75], 4, 4),
            "velocity": 50.0,          
            "duration": 0.25,
        },
        {
            "sound": "909ch",
            # "beats": repeat([0.25, 0.5, 0.75], 1, 8),
            # "beats": repeat([0.5, 1.5], 2, 4),
            # "beats": repeat([0.5, 0.75, 1.25, 1.75], 2, 4),
            "beats": repeat([0.0, 0.5, 0.75], 1, 16),
            "velocity": 15,
            "duration": 0.25,
        },
    ]
    return patterns


if __name__ == "__main__":
    patterns = generate_patterns()
    with open("patterns.json", "w") as f:
        json.dump(patterns, f, indent=4)
    print("Generated patterns.json")
