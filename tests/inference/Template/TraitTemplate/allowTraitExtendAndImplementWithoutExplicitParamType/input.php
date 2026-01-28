<?php
/**
 * @template T
 */
trait ValueObjectTrait
{
    /**
     * @psalm-var ?T
     */
    protected $value;

    /**
     * @psalm-param T $value
     *
     * @param $value
     */
    private function setValue($value): void {
        $this->validate($value);

        $this->value = $value;
    }

    /**
     * @psalm-param T $value
     *
     * @param $value
     */
    abstract protected function validate($value): void;
}

final class StringValidator {
    /**
     * @template-use ValueObjectTrait<string>
     */
    use ValueObjectTrait;

    protected function validate($value): void
    {
        if (strlen($value) > 30) {
            throw new \Exception("bad");
        }
    }
}