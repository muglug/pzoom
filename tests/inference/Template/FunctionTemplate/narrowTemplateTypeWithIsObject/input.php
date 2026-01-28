<?php
function takesObject(object $object): void {}

/**
 * @template T as mixed
 * @param T $value
 */
function example($value): void {
    if (is_object($value)) {
        takesObject($value);
    }
}