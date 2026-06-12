<?php
function crashes(): void {
    if (rand(0,1)) {
        $dt = new \DateTime;
    }
    /**
     * @psalm-suppress PossiblyUndefinedVariable
     * @psalm-suppress MixedArgument
     */
    assert($dt);
}
