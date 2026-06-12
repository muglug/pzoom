<?php

/** @param int<1, max> $count */
function needPositive(int $count): int {
    return $count;
}

/** @param int<0, max> $min_count */
function narrowToPositive(int $min_count): int {
    if ($min_count) {
        return needPositive($min_count);
    }
    return 0;
}

/** @param int<-5, -1>|int<1, 5> $nonZero */
function takeNonZero(int $nonZero): int {
    return $nonZero;
}

/** @param int<-5, 5> $n */
function splitAroundZero(int $n): int {
    if ($n) {
        return takeNonZero($n);
    }
    return 0;
}
