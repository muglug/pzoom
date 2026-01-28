<?php
/**
 * @template Key
 * @template Element
 * @psalm-param iterable<Key, Element> $input
 * @psalm-return Iterator<Key, Element>
 */
function to_iterator(iterable $input): Iterator
{
    if (\is_array($input)) {
        return new \ArrayIterator($input);
    } elseif ($input instanceof Iterator) {
        return $input;
    } else {
        return new \IteratorIterator($input);
    }
}