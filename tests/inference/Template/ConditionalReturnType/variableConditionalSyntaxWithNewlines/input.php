<?php
/**
 * @psalm-return (
 *      $i is 0
 *      ? string
 *      : (
 *          $i is 1
 *          ? int
 *          : bool
 *      )
 *  )
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