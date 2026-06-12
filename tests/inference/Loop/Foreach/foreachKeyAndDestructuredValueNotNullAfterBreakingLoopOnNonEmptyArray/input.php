<?php
/** @param non-empty-array<int, array{int, string|null}> $argument_map */
function f(array $argument_map, int $offset): ?string {
    $start_pos = null;
    $end_pos = null;
    $reference = null;
    foreach ($argument_map as $start_pos => [$end_pos, $possible_reference]) {
        if ($offset < $start_pos) {
            break;
        }
        if ($offset > $end_pos) {
            continue;
        }
        $reference = $possible_reference;
    }
    if ($reference === null || $start_pos === null || $end_pos === null) {
        return null;
    }
    return $reference;
}
