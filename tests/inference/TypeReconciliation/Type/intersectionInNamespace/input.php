<?php
namespace NS;
use Countable;

class Item {}
/**
 * @var iterable<Item>&Countable $collection
 */
$collection = [];
count($collection);

/**
 * @param iterable<Item>&Countable $collection
 */
function mycount($collection): int {
    return count($collection);
}
mycount($collection);