<?php
/**
 * @param object&callable():int $object
 */
function takesCallableObject(object $object): int {
    return $object();
}
