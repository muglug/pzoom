<?php
trait ForbidCloning2 {
    final protected function __clone()
    {
    }
}

abstract class AbstractCtx {
    use ForbidCloning2;
}

final class ForkCtx extends AbstractCtx {
    public function run(): string { return 'x'; }
}

function f(ForkCtx $c): string { return $c->run(); }
