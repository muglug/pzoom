<?php
function f(SimpleXMLElement $config_xml): void {
    if (isset($config_xml->enableExtensions) && isset($config_xml->enableExtensions->extension)) {
        foreach ($config_xml->enableExtensions->extension as $extension) {
            assert(isset($extension["name"]));
            $name = (string) $extension["name"];
            echo $name;
        }
    }
}
