<?php
/**
 * @param list<0> $currentIndexes
 */
function cartesianProduct(array $currentIndexes): void {
    while (rand(0, 1)) {
        array_map(
            function ($index) { echo $index; },
            $currentIndexes
        );

        /** @psalm-suppress PossiblyUndefinedArrayOffset */
        $currentIndexes[0]++;
    }
}
