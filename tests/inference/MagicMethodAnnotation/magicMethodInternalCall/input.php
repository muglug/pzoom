<?php
/**
 * @method I[] work()
 */
class I {
    function __call(string $method, array $args) { return [new I, new I]; }

    function zugzug(): void {
        echo count($this->work());
    }
}
