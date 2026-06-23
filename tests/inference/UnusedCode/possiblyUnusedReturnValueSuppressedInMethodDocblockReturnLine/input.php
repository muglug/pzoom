<?php
final class C {
    /**
     * Some description.
     *
     * @return false|null
     * @psalm-suppress PossiblyUnusedReturnValue unused but seems important
     */
    public function analyze(): ?bool {
        return null;
    }

    public function run(): void {
        $this->analyze();
    }
}

$c = new C();
$c->run();
