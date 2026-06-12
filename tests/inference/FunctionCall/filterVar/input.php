<?php
function namedArgs(): string {
    $a = filter_var("a", options: FILTER_FORCE_ARRAY);
    return $a[0];
}

function filterInt(string $s) : int {
    $filtered = filter_var($s, FILTER_VALIDATE_INT);
    if ($filtered === false) {
        return 0;
    }
    return $filtered;
}
function filterNullableInt(string $s) : ?int {
    return filter_var($s, FILTER_VALIDATE_INT, ["options" => ["default" => null]]);
}
function filterIntWithDefault(string $s) : int {
    return filter_var($s, FILTER_VALIDATE_INT, ["options" => ["default" => 5]]);
}
function filterBool(string $s) : bool {
    return filter_var($s, FILTER_VALIDATE_BOOLEAN);
}
function filterNullableBool(string $s) : ?bool {
    return filter_var($s, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE);
}
function filterNullableBoolWithFlagsArray(string $s) : ?bool {
    return filter_var($s, FILTER_VALIDATE_BOOLEAN, ["flags" => FILTER_NULL_ON_FAILURE]);
}
function filterFloat(string $s) : float {
    $filtered = filter_var($s, FILTER_VALIDATE_FLOAT);
    if ($filtered === false) {
        return 0.0;
    }
    return $filtered;
}
function filterFloatWithDefault(string $s) : float {
    return filter_var($s, FILTER_VALIDATE_FLOAT, ["options" => ["default" => 5.0]]);
}

/**
 * @param mixed $c
 * @return int<1, 100>|stdClass|array<never, never>
 */
function filterNumericIntWithDefault($c) {
    if (is_numeric($c)) {
        return filter_var($c, FILTER_VALIDATE_INT, [
         "options" => [
            "default"   => new stdClass(),
            "min_range" => 1,
            "max_range" => 100,
        ],
        ]);
    }

    return array();
}
