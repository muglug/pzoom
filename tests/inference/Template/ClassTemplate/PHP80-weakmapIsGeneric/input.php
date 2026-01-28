<?php
/** @param WeakMap<Throwable,int> $wm */
function isCountable(WeakMap $wm): int {
    return count($wm);
}

/**
 * @param WeakMap<Throwable,int> $wm
 * @return array{Throwable,int}
 */
function isTraverable(WeakMap $wm): array {
    foreach ($wm as $k => $v) {
        return [$k, $v];
    }
    throw new RuntimeException;
}

/**
 * @param WeakMap<Throwable,int> $wm
 * @return Traversable<Throwable,int>
 */
function hasAggregateIterator(WeakMap $wm): Traversable {
    return $wm->getIterator();
}

/**
 * @param WeakMap<Throwable,int> $wm
 */
function readsLikeAnArray(WeakMap $wm): int {
    $ex = new Exception;
    if (isset($wm[$ex])) {
        return $wm[$ex];
    }
    throw new RuntimeException;
}

/**
 * @param WeakMap<Throwable,int> $wm
 */
function writesLikeAnArray(WeakMap $wm): void {
    $ex = new Exception;
    $wm[$ex] = 42;
}
                