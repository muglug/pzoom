<?php
function accept(Generator $gen): void {}

accept(
    (function() {
        yield;
        return 42;
    })()
);
