<?php
/**
 * @psalm-return ($idOnly is true ? array<int> : array<stdClass>)
 */
function test(bool $idOnly = false) {
    if ($idOnly) {
        return [0, 1];
    }

    return [new stdClass(), new stdClass()];
}