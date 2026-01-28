<?php
class App {
    public function bar(int $i) : bool {
        return $i === 5;
    }
}

/** @psalm-suppress MixedArgument, MissingParamType */
function bar(App $foo, $arr) : void {
    /** @psalm-suppress TypeDoesNotContainNull */
    if ($foo === null || $foo->bar($arr)) {
        return;
    }
}