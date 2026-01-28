<?php
/**
 * @psalm-immutable
 */
class Address
{
    private $line1;
    private $line2;
    private $city;

    public function __construct(
        string $line1,
        ?string $line2,
        string $city
    ) {
        $this->line1 = $line1;
        $this->line2 = $line2;
        $this->city = $city;
    }

    public function __toString()
    {
        $parts = [
            $this->line1,
            $this->line2 ?? "",
            $this->city,
        ];

        // Remove empty parts
        $parts = \array_map("trim", $parts);
        $parts = \array_filter($parts, "strlen");
        $parts = \array_map(function(string $s) { return $s;}, $parts);

        return \implode(", ", $parts);
    }
}
