<?php
/**
 * @template T
 * @template TOptionalClosure as (callable():T)|null
 * @param TOptionalClosure $cb
 * @return (TOptionalClosure is null ? int : T)
 */
function f($cb) {
    if (is_callable($cb)) {
        return $cb();
    }

    return 1;
}