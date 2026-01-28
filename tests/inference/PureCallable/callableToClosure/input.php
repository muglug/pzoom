<?php
/**
 * @return pure-callable
 */
function foo() {
    return
        /**
         * @psalm-pure
         */
        function(string $a): string {
            return $a . "blah";
        };
}
