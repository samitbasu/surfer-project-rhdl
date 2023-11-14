<div class=tomldoc>

<details open>

<summary id="SurferConfig">SurferConfig</summary>


## Summary
```toml

[layout]
<SurferLayout>

[theme]
<SurferTheme>

[gesture]
<SurferGesture>

[default_signal_name_type]
<SignalNameType>

[default_clock_highlight_type]
<ClockHighlightType>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>layout</span> <span class=tomldoc_type> <a href="#SurferLayout">SurferLayout</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>theme</span> <span class=tomldoc_type> <a href="#SurferTheme">SurferTheme</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>gesture</span> <span class=tomldoc_type> <a href="#SurferGesture">SurferGesture</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>default_signal_name_type</span> <span class=tomldoc_type> <a href="#SignalNameType">SignalNameType</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>default_clock_highlight_type</span> <span class=tomldoc_type> <a href="#ClockHighlightType">ClockHighlightType</a> </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

<details >

<summary id="ClockHighlightType">ClockHighlightType</summary>


### One of these strings:
- `"Line"`
- `"Cycle"`
- `"None"`

</details>

</div>
<div class=tomldoc>

<details >

<summary id="SignalNameType">SignalNameType</summary>


### One of these strings:
- `"Local"`
- `"Unique"`
- `"Global"`

</details>

</div>
<div class=tomldoc>

<details >

<summary id="SurferGesture">SurferGesture</summary>


## Summary
```toml
size = …
deadzone = …

[style]
<SurferLineStyle>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>style</span> <span class=tomldoc_type> <a href="#SurferLineStyle">SurferLineStyle</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>size</span> <span class=tomldoc_type> f32 </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>deadzone</span> <span class=tomldoc_type> f32 </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

<details >

<summary id="SurferLineStyle">SurferLineStyle</summary>


## Summary
```toml
width = …

[color]
<Color32>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>color</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>width</span> <span class=tomldoc_type> f32 </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

Did not find type with id (Id("42:224:2507"), [])

</div>
<div class=tomldoc>

<details >

<summary id="SurferTheme">SurferTheme</summary>


## Summary
```toml
colors = {key: <Color32>, …}
linewidth = …
alt_frequency = …

# The color used for text across the UI
[foreground]
<Color32>

# The color of borders between UI elements
[border_color]
<Color32>

# The colors used for the background and text of the wave view
[canvas_colors]
<ThemeColorTriple>

# The colors used for most UI elements not on the signal canvas
[primary_ui_color]
<ThemeColorPair>

# The colors used for the variable and value list, as well as secondary elements
# like text fields
[secondary_ui_color]
<ThemeColorPair>

# The color used for selected ui elements such as the currently selected hierarchy
[selected_elements_colors]
<ThemeColorPair>

[accent_info]
<ThemeColorPair>

[accent_warn]
<ThemeColorPair>

[accent_error]
<ThemeColorPair>

[cursor]
<SurferLineStyle>

[clock_highlight_line]
<SurferLineStyle>

[clock_highlight_cycle]
<Color32>

[signal_default]
<Color32>

[signal_highimp]
<Color32>

[signal_undef]
<Color32>

[signal_dontcare]
<Color32>

[signal_weak]
<Color32>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>foreground</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>

The color used for text across the UI

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>border_color</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>

The color of borders between UI elements

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>canvas_colors</span> <span class=tomldoc_type> <a href="#ThemeColorTriple">ThemeColorTriple</a> </span></h3>

The colors used for the background and text of the wave view

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>primary_ui_color</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>

The colors used for most UI elements not on the signal canvas

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>secondary_ui_color</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>

The colors used for the variable and value list, as well as secondary elements
like text fields

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>selected_elements_colors</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>

The color used for selected ui elements such as the currently selected hierarchy

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>accent_info</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>accent_warn</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>accent_error</span> <span class=tomldoc_type> <a href="#ThemeColorPair">ThemeColorPair</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>cursor</span> <span class=tomldoc_type> <a href="#SurferLineStyle">SurferLineStyle</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>clock_highlight_line</span> <span class=tomldoc_type> <a href="#SurferLineStyle">SurferLineStyle</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>clock_highlight_cycle</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>signal_default</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>signal_highimp</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>signal_undef</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>signal_dontcare</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>signal_weak</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>colors</span> <span class=tomldoc_type> Map[String =&gt; <a href="#Color32">Color32</a>] </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>linewidth</span> <span class=tomldoc_type> f32 </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>alt_frequency</span> <span class=tomldoc_type> usize </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

<details >

<summary id="ThemeColorPair">ThemeColorPair</summary>


## Summary
```toml

[foreground]
<Color32>

[background]
<Color32>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>foreground</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>background</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

<details >

<summary id="ThemeColorTriple">ThemeColorTriple</summary>


## Summary
```toml

[foreground]
<Color32>

[background]
<Color32>

[alt_background]
<Color32>
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>foreground</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>background</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>alt_background</span> <span class=tomldoc_type> <a href="#Color32">Color32</a> </span></h3>


</div>


</details>

</div>
<div class=tomldoc>

<details >

<summary id="SurferLayout">SurferLayout</summary>


## Summary
```toml
# Flag to show/hide the hierarchy view
show_hierarchy = true|false
# Flag to show/hide the menu
show_menu = true|false
# Initial window height
window_height = …
# Initial window width
window_width = …
```
<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>show_hierarchy</span> <span class=tomldoc_type> bool </span></h3>

Flag to show/hide the hierarchy view

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>show_menu</span> <span class=tomldoc_type> bool </span></h3>

Flag to show/hide the menu

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>window_height</span> <span class=tomldoc_type> usize </span></h3>

Initial window height

</div>

<div class=field_doc>

<h3 class=struct_field> <span class=tomldoc_param_name>window_width</span> <span class=tomldoc_type> usize </span></h3>

Initial window width

</div>


</details>

</div>