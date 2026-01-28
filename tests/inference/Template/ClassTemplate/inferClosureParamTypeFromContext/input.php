<?php
/**
 * @template E
 */
interface Collection {
    /**
     * @template R
     * @param callable(E):R $action
     * @return Collection<R>
     */
    function map(callable $action): self;
}

/**
 * @template T
 */
interface Optional {
    /**
     * @return T
     */
    function get();
}

/**
 * @param Collection<Optional<string>> $collection
 * @return Collection<string>
 */
function expandOptions(Collection $collection) : Collection {
    return $collection->map(
        function ($optional) {
            return $optional->get();
        }
    );
}