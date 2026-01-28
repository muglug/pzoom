<?php
/**
 * @return pure-callable
 */
function foo() {
    return
        /**
         * @psalm-pure
         */
        fn(string $a): string => $a . "blah";
}
