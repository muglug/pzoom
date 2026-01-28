<?php
/** @template T */
interface F {}

/** @param F<mixed> $f */
function takesFMixed(F $f) : void {}

function sendsF(F $f) : void {
    takesFMixed($f);
}