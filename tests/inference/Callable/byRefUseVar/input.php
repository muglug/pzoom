<?php
/** @return void */
function run_function(\Closure $fnc) {
    $fnc();
}

/**
 * @return void
 */
function f() {
    $data = 0;
    run_function(
        /**
         * @return void
         */
        function() use(&$data) {
            $data = 1;
        }
    );
    echo $data;
}

f();
