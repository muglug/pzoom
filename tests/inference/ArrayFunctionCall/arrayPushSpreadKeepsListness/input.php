<?php
abstract class AtomicV {}

/** @return non-empty-list<AtomicV> */
function expandV(AtomicV $a): array { return [$a]; }

/**
 * @param list<AtomicV> $atomics
 * @return list<AtomicV>
 */
function partsV(array $atomics): array {
    $out = [];
    foreach ($atomics as $atomic) {
        if (rand(0, 1)) {
            $out[] = $atomic;
            continue;
        }
        $expanded = expandV($atomic);
        array_push($out, ...$expanded);
    }
    return $out;
}
