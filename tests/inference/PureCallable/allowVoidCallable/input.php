<?php
/**
 * @param pure-callable():void $p
 */
function doSomething($p): void {}
doSomething(
    /**
     * @psalm-pure
     */
    function(): bool { return false; }
);
