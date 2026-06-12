<?php
class IncludeCollector {
    /**
     * @template T
     * @param callable():T $f
     * @return T
     */
    public function runAndCollect(callable $f) {
        return $f();
    }
}

function collect(IncludeCollector $ic, string $path): array {
    return $ic->runAndCollect(
        static fn(): array =>
            /**
             * @psalm-suppress UnresolvableInclude
             * @var string[]
             */
            require $path,
    );
}
