<?php
/**
 * @template T of mixed|false|null
 * @param T $i
 * @return (T is false ? no-return : T is null ? no-return : T)
 */
function orThrow($i) {
    if ($i === false || $i === null) {
        throw new RuntimeException("Example");
    }
    return $i;
}