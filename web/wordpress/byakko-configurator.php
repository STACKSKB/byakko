<?php
/**
 * Plugin Name: Byakko Configurator
 * Description: Embed the self-contained Byakko keyboard configurator with a shortcode.
 * Version: 0.1.2
 * License: GPL-3.0-or-later
 * License URI: https://www.gnu.org/licenses/gpl-3.0.html
 * Text Domain: byakko-configurator
 */

// SPDX-License-Identifier: GPL-3.0-or-later
if ( ! defined( 'ABSPATH' ) ) {
    exit;
}

function byakko_configurator_shortcode() {
    $bundle = __DIR__ . '/index.html';
    $url = plugins_url( 'index.html', __FILE__ );
    if ( is_file( $bundle ) ) {
        $modified = filemtime( $bundle );
        if ( false !== $modified ) {
            $url = add_query_arg( 'v', (string) $modified, $url );
        }
    }
    $embed_url = esc_url( add_query_arg( 'embed', 'wordpress', $url ) );
    $url = esc_url( $url );
    wp_enqueue_script( 'byakko-configurator-embed', plugins_url( 'embed.js', __FILE__ ), array(), '0.1.2', true );

    $linux_url = esc_url( plugins_url( 'linux-requirements.html', __FILE__ ) );
    return '<div class="byakko-configurator-embed"><p data-byakko-linux-requirements hidden style="margin:16px 22px">Linux requires permission to access the keyboard. <a href="' . $linux_url . '" target="_blank" rel="noopener noreferrer">Read the Linux WebHID requirements</a> before connecting.</p><iframe data-byakko-configurator src="' . $embed_url . '" title="'
        . esc_attr( 'Byakko keyboard configurator' )
        . '" allow="hid; display-capture" style="display:block;width:100%;height:85vh;border:0"></iframe>'
        . '<p style="margin:8px 22px;font-size:12px"><a href="' . $url . '" target="_blank" rel="noopener noreferrer">'
        . esc_html( 'Open configurator in a new tab' ) . '</a></p></div>';
}

add_shortcode( 'byakko_configurator', 'byakko_configurator_shortcode' );
