<?php
/** @template-covariant T */
interface Container {
    /** @return T */
    public function get();
}

/**
 * @template T
 * @param array<Container<T>> $containers
 * @return T
 */
function unwrap(array $containers) {
    return array_map(
        fn($container) => $container->get(),
        $containers
    )[0];
}

/**
 * @param array<Container<int>|Container<string>> $typed_containers
 */
function takesDifferentTypes(array $typed_containers) : void {
    $ret = unwrap($typed_containers);

    if (is_string($ret)) {}
    if (is_int($ret)) {}
}