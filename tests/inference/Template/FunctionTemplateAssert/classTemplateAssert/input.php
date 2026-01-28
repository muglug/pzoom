<?php
/**
 * @template ActualFieldType
 */
final class FieldValue
{
    /** @var ActualFieldType */
    public $value;

    /** @param ActualFieldType $value */
    public function __construct($value) {
        $this->value = $value;
    }
}

/**
 * @template FieldDefinitionType
 *
 * @param string|bool|int|null $value
 * @param FieldDefinition<FieldDefinitionType> $definition
 *
 * @return FieldValue<FieldDefinitionType>
 */
function fromScalarAndDefinition($value, FieldDefinition $definition) : FieldValue
{
    $definition->assertAppliesToValue($value);

    return new FieldValue($value);
}

/**
 * @template ExpectedFieldType
 */
final class FieldDefinition
{
    /**
     * @param mixed $value
     * @psalm-assert ExpectedFieldType $value
     */
    public function assertAppliesToValue($value): void
    {
      throw new \Exception("bad");
    }
}