<?php
/**
 * @param \Traversable<int, int>&\Countable $object
 */
function doSomethingUseful($object) : void {
    echo count($object);
    foreach ($object as $foo) {}
}
