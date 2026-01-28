<?php
function crashes(): void {
    if (rand(0,1)) {
        $dt = new \DateTime;
    }
    /**
     * @psalm-suppress UndefinedVariable
     * @psalm-suppress MixedArgument
     */
    assert($dt);
}
