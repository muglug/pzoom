<?php
final class Foo {
    /**
     * @psalm-suppress MixedArgument
     */
    public function bar(string $request): void {
        /** @var mixed $action */
        $action = "";
        $this->{"execute" . ucfirst($action)}($request);
    }
}

(new Foo)->bar("request");
