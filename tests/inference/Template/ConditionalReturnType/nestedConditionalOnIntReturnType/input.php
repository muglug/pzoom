<?php
/**
 * @template T as int
 * @param T $i
 * @psalm-return (T is 0 ? string : (T is 1 ? int : bool))
 */
function getDifferentType(int $i) {
    if ($i === 0) {
        return "hello";
    }

    if ($i === 1) {
        return 5;
    }

    return true;
}