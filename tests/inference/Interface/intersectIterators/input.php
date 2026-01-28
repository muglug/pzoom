<?php
interface A {} function takesA(A $p): void {}
interface B {} function takesB(B $p): void {}

/** @psalm-param iterable<A>&iterable<B> $i */
function takesIntersectionOfIterables(iterable $i): void {
    foreach ($i as $c) {
        takesA($c);
        takesB($c);
    }
}

/** @psalm-param iterable<A&B> $i */
function takesIterableOfIntersections(iterable $i): void {
    foreach ($i as $c) {
        takesA($c);
        takesB($c);
    }
}
