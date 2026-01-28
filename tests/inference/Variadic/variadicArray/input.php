<?php
/**
 * @param int ...$a_list
 * @return array<array-key, int>
 */
function f(int ...$a_list) {
    return array_map(
        /**
         * @return int
         */
        function (int $a) {
            return $a + 1;
        },
        $a_list
    );
}

f(1);
f(1, 2);
f(1, 2, 3);

/**
 * @param string ...$a_list
 * @return void
 */
function g(string ...$a_list) {
}
