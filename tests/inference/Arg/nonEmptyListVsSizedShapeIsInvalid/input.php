<?php
class Pair20 {}
/** @param list{Pair20, Pair20} $params */
function takesPair(array $params): void { echo count($params); }

/** @param non-empty-list<Pair20> $list */
function caller(array $list): void {
    /** @psalm-suppress InvalidArgument */
    takesPair($list);
}
