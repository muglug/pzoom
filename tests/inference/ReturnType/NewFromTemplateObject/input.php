<?php
/** @psalm-consistent-constructor */
class AggregateResult {}

/**
 * @template T as AggregateResult
 * @param T $type
 * @return T
 */
function aggregate($type) {
    $t = new $type;
    return $t;
}
