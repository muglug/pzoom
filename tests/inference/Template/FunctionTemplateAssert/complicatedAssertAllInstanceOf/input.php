<?php
/**
 * @template T
 *
 * @psalm-assert-if-true iterable<mixed,T> $i
 *
 * @param iterable<mixed,mixed> $i
 * @param class-string<T>|interface-string<T> $type
 */
function allInstanceOf(iterable $i, string $type): bool {
    foreach ($i as $elt) {
        if (!$elt instanceof $type) {
            return false;
        }
    }
    return true;
}

interface IBlogPost { public function getId(): int; }

function getData(): iterable {
    return [];
}

$data = getData();

assert(allInstanceOf($data, IBlogPost::class));

foreach ($data as $post) {
    echo $post->getId();
}