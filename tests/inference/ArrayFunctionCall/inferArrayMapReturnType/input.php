<?php
/** @return array<string> */
function Foo(DateTime ...$dateTimes) : array {
    return array_map(
        function ($dateTime) {
            return ($dateTime->format("c"));
        },
        $dateTimes
    );
}
