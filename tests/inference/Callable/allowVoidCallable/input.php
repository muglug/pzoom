<?php
/**
 * @param callable():void $p
 */
function doSomething($p): void {}
doSomething(function(): bool { return false; });
