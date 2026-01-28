<?php
function withArrayWalk(array &$val): void {
    array_walk($val, /** @param mixed $arg */ function (&$arg): void {});
}
function withArrayWalkRecursive(array &$val): void {
    array_walk_recursive($val, /** @param mixed $arg */ function (&$arg): void {});
}
