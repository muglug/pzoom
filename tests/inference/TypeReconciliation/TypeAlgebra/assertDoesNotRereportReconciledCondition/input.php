<?php

class Location {}

class ConstantStorage {
    public ?Location $location = null;
}

function checkGuardedAssert(ConstantStorage $const_storage, int $item): void {
    if ($const_storage->location !== null && $item !== 0) {
        assert($item < 5);
        echo 'x';
    }
}

/** @param list<int> $items */
function checkInLoop(ConstantStorage $const_storage, array $items): void {
    foreach ($items as $item) {
        if ($const_storage->location !== null && $item !== 0) {
            assert($item < 5);
            echo 'x';
        }
    }
}
