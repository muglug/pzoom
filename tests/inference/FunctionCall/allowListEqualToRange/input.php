<?php
/** @param array<int, int> $two */
function collectCommit(array $one, array $two) : void {
    if ($one && array_values($one) === array_values($two)) {}
}
