<?php

interface HasAttributes
{
    /** @return array<string, mixed> */
    public function getAttributes(): array;
}

// Concrete ancestor re-declares the method with only the native `array` hint and
// no docblock. The documented return type must still be inherited from the
// interface rather than being shadowed by the bare signature type.
abstract class NodeAbstract implements HasAttributes
{
    public function getAttributes(): array
    {
        return [];
    }
}

final class Node extends NodeAbstract {}

/** @param array<string, mixed> $attributes */
function consume(array $attributes): void {}

function run(Node $node): void
{
    consume($node->getAttributes());
}
