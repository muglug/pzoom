<?php
/** @return array<string> */
function Foo(DateTime ...$dateTimes) : array {
    return array_map(
        fn ($dateTime) => ($dateTime->format("c")),
        $dateTimes
    );
}
