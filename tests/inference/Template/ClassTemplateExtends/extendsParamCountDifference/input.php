<?php
/**
 * @template E
 * @implements \Iterator<int,E>
 */
abstract class Collection implements \Iterator {}

/**
 * @param Collection<string> $collection
 * @return \Iterator<int,string>
 */
function foo(Collection $collection) {
    return $collection;
}