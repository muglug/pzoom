<?php
/**
 * @return array { // Array with comments
 *     // Array with single quoted keys
 *     'single quote keys': array {           // Single quoted key
 *         'single_quote_key//1': int,        // Single quoted key with //
 *         'single_quote_key\'//2': string,   // Single quoted key with ' and //
 *         'single_quote_key\'//\'3': bool,   // Single quoted key with 2x ' and //
 *         'single_quote_key"//"4': float,    // Single quoted key with 2x " and //
 *         'single_quote_key"//\'5': array {  // Single quoted key with ', " and //
 *             'single_quote_key//5//1': int, // Single quoted key with 2x //
 *         },
 *         // 'commented_out_array_element//1': int
 *         'single_quote_key//no_whitespace':int,//Single quoted key without whitespace
 *     },
 *     // Array with double quoted keys
 *     "double quote keys": array {           // Double quoted key
 *         "double_quote_key//1": int,        // Double quoted key with //
 *         "double_quote_key'//2": string,    // Double quoted key with ' and //
 *         "double_quote_key\"//\"3": bool,   // Double quoted key with 2x ' and //
 *         "double_quote_key'//'4": float,    // Double quoted key with 2x " and //
 *         "double_quote_key\"//'5": array {  // Double quoted key with ', " and //
 *             "double_quote_key//5//1": int, // Double quoted key with 2x //
 *         },
 *         // "commented_out_array_element//1": int
 *         "double_quote_key//no_whitespace":int,//Double quoted key without whitespace
 *     },
 * }
 */
function f(): array
{
    return [
        'single quote keys' => [
            'single_quote_key//1' => 1,
            'single_quote_key\'//2' => 'string',
            'single_quote_key\'//\'3' => true,
            'single_quote_key"//"4' => 0.1,
            'single_quote_key"//\'5' => [
                'single_quote_key//5//1' => 1,
            ],
            'single_quote_key//no_whitespace' => 1
        ],
        "double quote keys" => [
            "double_quote_key//1" => 1,
            "double_quote_key'//2" => 'string',
            "double_quote_key\"//\"3" => true,
            "double_quote_key'//'4" => 0.1,
            "double_quote_key\"//'5" => [
                "double_quote_key//5//1" => 1,
            ],
            "double_quote_key//no_whitespace" => 1
        ],
    ];
}
