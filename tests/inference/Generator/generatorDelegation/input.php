<?php
/**
 * @return Generator<int, int, mixed, int>
 */
function count_to_ten(): Generator {
    yield 1;
    yield 2;
    yield from [3, 4];
    yield from new ArrayIterator([5, 6]);
    yield from seven_eight();
    return yield from nine_ten();
}

/**
 * @return Generator<int, int>
 */
function seven_eight(): Generator {
    yield 7;
    yield from eight();
}

/**
 * @return Generator<int,int>
 */
function eight(): Generator {
    yield 8;
}

/**
 * @return Generator<int,int, mixed, int>
 */
function nine_ten(): Generator {
    yield 9;
    return 10;
}

$gen = count_to_ten();
foreach ($gen as $num) {
    echo "$num ";
}
$gen2 = $gen->getReturn();
