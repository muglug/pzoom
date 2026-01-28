<?php
/** @psalm-return list<int> */
function makeAList(int $ofThisInteger): array {
    return array_filter([$ofThisInteger]);
}
