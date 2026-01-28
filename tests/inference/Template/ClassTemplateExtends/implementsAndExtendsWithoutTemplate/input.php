<?php
/**
 * @template E
 */
interface Collection
{
    /**
     * @return array<E>
     */
    function toArray();
}

/**
 * @implements Collection<int>
 */
class IntCollection implements Collection
{
    function toArray() {
        return ["foo"];
    }
}
