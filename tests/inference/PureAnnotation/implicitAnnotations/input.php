<?php
abstract class Foo {
    private array $options;
    private array $defaultOptions;

    function __construct(array $options) {
        $this->setOptions($options);
        $this->setDefaultOptions($this->getOptions());
    }

    function getOptions(): array {
        return $this->options;
    }

    public final function setOptions(array $options): void {
        $this->options = $options;
    }

    public final function setDefaultOptions(array $defaultOptions): void {
        $this->defaultOptions = $defaultOptions;
    }
}
