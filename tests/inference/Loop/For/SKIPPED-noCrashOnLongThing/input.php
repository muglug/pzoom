<?php
/**
 * @param list<array{a: array{int, int}}> $data
 */
function makeData(array $data) : array {
    while (rand(0, 1)) {
        while (rand(0, 1)) {
            while (rand(0, 1)) {
                if (rand(0, 1)) {
                    continue;
                }

                /** @psalm-suppress PossiblyUndefinedArrayOffset */
                $data[0]["a"] = array_merge($data[0]["a"], $data[0]["a"]);
            }
        }
    }

    return $data;
}
