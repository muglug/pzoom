<?php
/**
 * @template T as object
 * @template S as object
 * @param S&T $item
 * @return T&S
 */
function filter(object $item) {
    return $item;
}