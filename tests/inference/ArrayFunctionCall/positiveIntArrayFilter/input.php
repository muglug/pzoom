<?php
/**
 * @param numeric $a
 * @param positive-int $positiveOne
 * @param int<0,12> $d
 * @param int<1,12> $f
 * @psalm-return array{a: numeric, b?: int, c: positive-int, d?: int<0, 12>, f: int<1,12>}
 */
function makeAList($a, int $anyInt, int $positiveOne, int $d, int $f): array {
    return array_filter(["a" => "1", "b" => $anyInt, "c" => $positiveOne, "d" => $d, "f" => $f]);
}
