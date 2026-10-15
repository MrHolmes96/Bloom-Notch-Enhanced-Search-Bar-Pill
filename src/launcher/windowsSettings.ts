/**
 * Windows Settings and Control Panel entries for the search bar's Windows
 * Settings plugin: [name, area, kind, command, extra search words], where kind
 * is "A" (Settings app), "C" (Control Panel) or "T" (a Control Panel task).
 *
 * Generated from Flow Launcher's WindowsSettings plugin data, which was
 * ported from Microsoft PowerToys. Both are MIT licensed:
 * Copyright (c) Microsoft Corporation; Copyright (c) Flow Launcher contributors.
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files, to deal in the
 * Software without restriction, subject to including this notice. The
 * software is provided "as is", without warranty of any kind.
 */
export type WindowsSettingRow = [
	name: string,
	area: string,
	kind: "A" | "C" | "T",
	command: string,
	extra: string
];

export const WINDOWS_SETTINGS: WindowsSettingRow[] = [
	["Access work or school", "Accounts", "A", "ms-settings:workplace", "workplace"],
	["Email and app accounts", "Accounts", "A", "ms-settings:emailandaccounts", ""],
	["Family and other people", "Accounts", "A", "ms-settings:otherusers", "other users"],
	["Set up a kiosk", "Accounts", "A", "ms-settings:assignedaccess", "assigned access"],
	["Sign-in options", "Accounts", "A", "ms-settings:signinoptions", ""],
	["Sign-in options - Dynamic lock", "Accounts", "A", "ms-settings:signinoptions-dynamiclock", ""],
	["Sync your settings", "Accounts", "A", "ms-settings:sync", ""],
	[
		"Windows Hello setup - Face",
		"Accounts",
		"A",
		"ms-settings:signinoptions-launchfaceenrollment",
		""
	],
	[
		"Windows Hello setup - Fingerprint",
		"Accounts",
		"A",
		"ms-settings:signinoptions-launchfingerprintenrollment",
		""
	],
	["Your info", "Accounts", "A", "ms-settings:yourinfo", ""],
	["Apps and Features", "Apps", "A", "ms-settings:appsfeatures", ""],
	["App features", "Apps", "A", "ms-settings:appsfeatures-app", ""],
	["Apps for websites", "Apps", "A", "ms-settings:appsforwebsites", ""],
	["Default apps", "Apps", "A", "ms-settings:defaultapps", ""],
	["Manage optional features", "Apps", "A", "ms-settings:optionalfeatures", ""],
	["Offline Maps", "Apps", "A", "ms-settings:maps", ""],
	["Offline Maps - Download maps", "Apps", "A", "ms-settings:maps-downloadmaps", ""],
	["Startup apps", "Apps", "A", "ms-settings:startupapps", ""],
	["Video playback", "Apps", "A", "ms-settings:videoplayback", ""],
	["Notifications", "Cortana", "A", "ms-settings:cortana-notifications", ""],
	["More details", "Cortana", "A", "ms-settings:cortana-moredetails", ""],
	["Permissions and history", "Cortana", "A", "ms-settings:cortana-permissions", ""],
	["Windows search", "Cortana", "A", "ms-settings:cortana-windowssearch", ""],
	["Cortana - Language", "Cortana", "A", "ms-settings:cortana-language", "talk"],
	["Cortana", "Cortana", "A", "ms-settings:cortana", ""],
	["Talk to Cortana", "Cortana", "A", "ms-settings:cortana-talktocortana", ""],
	["AutoPlay", "Devices", "A", "ms-settings:autoplay", ""],
	["Bluetooth", "Devices", "A", "ms-settings:bluetooth", ""],
	["Connected Devices", "Devices", "A", "ms-settings:connecteddevices", ""],
	["Mouse and touchpad", "Devices", "A", "ms-settings:mousetouchpad", ""],
	["Pen and Windows Ink", "Devices", "A", "ms-settings:pen", ""],
	["Printers and scanners", "Devices", "A", "ms-settings:printers", ""],
	["Touchpad", "Devices", "A", "ms-settings:devices-touchpad", ""],
	["Typing", "Devices", "A", "ms-settings:typing", ""],
	["USB", "Devices", "A", "ms-settings:usb", ""],
	["Wheel", "Devices", "A", "ms-settings:wheel", ""],
	["Phone", "Phone", "A", "ms-settings:mobile-devices", "mobile devices"],
	["Audio", "Ease of access", "A", "ms-settings:easeofaccess-audio", "mono volume audio alerts"],
	["Closed captions", "Ease of access", "A", "ms-settings:easeofaccess-closedcaptioning", ""],
	[
		"Color filters",
		"Ease of access",
		"A",
		"ms-settings:easeofaccess-colorfilter",
		"inverted colors grayscale red-green blue-yellow green week red week deuteranopia protanopia tritanopia"
	],
	[
		"Mouse pointer",
		"Ease of access",
		"A",
		"ms-settings:easeofaccess-MousePointer",
		"touch feedback"
	],
	[
		"Display",
		"Ease of access",
		"A",
		"ms-settings:easeofaccess-display",
		"transparency animations scroll bars size"
	],
	["Eye control", "Ease of access", "A", "ms-settings:easeofaccess-eyecontrol", ""],
	["High contrast", "Ease of access", "A", "ms-settings:easeofaccess-highcontrast", ""],
	[
		"Keyboard",
		"Ease of access",
		"A",
		"ms-settings:easeofaccess-keyboard",
		"print screen shortcuts on-screen keys scroll lock caps lock num lock"
	],
	["Magnifier", "Ease of access", "A", "ms-settings:easeofaccess-magnifier", "zoom"],
	["Mouse", "Ease of access", "A", "ms-settings:easeofaccess-mouse", "keypad touch"],
	["Narrator", "Ease of access", "A", "ms-settings:easeofaccess-narrator", ""],
	[
		"Speech",
		"Ease of access",
		"A",
		"ms-settings:easeofaccess-speechrecognition",
		"recognition talk"
	],
	["Extras", "Extras", "A", "ms-settings:extras", ""],
	["Broadcasting", "Gaming", "A", "ms-settings:gaming-broadcasting", ""],
	["Game bar", "Gaming", "A", "ms-settings:gaming-gamebar", ""],
	["Game DVR", "Gaming", "A", "ms-settings:gaming-gamedvr", ""],
	["Game Mode", "Gaming", "A", "ms-settings:gaming-gamemode", ""],
	[
		"Playing a game full screen",
		"Gaming",
		"A",
		"ms-settings:quietmomentsgame",
		"quiet moments game"
	],
	["Xbox Networking", "Gaming", "A", "ms-settings:gaming-xboxnetworking", ""],
	["Settings home page", "Home page", "A", "ms-settings:", ""],
	["Audio and speech", "Mixed reality", "A", "ms-settings:holographic-audio", "holographic audio"],
	[
		"Environment",
		"Mixed reality",
		"A",
		"ms-settings:privacy-holographic-environment",
		"holographic environment"
	],
	[
		"Headset display",
		"Mixed reality",
		"A",
		"ms-settings:holographic-headset",
		"holographic headset"
	],
	[
		"Uninstall",
		"Mixed reality",
		"A",
		"ms-settings:holographic-management",
		"holographic management"
	],
	["Airplane mode", "Network and Internet", "A", "ms-settings:network-airplanemode", ""],
	["Proximity", "Network and Internet", "A", "ms-settings:proximity", ""],
	["Cellular and SIM", "Network and Internet", "A", "ms-settings:network-cellular", ""],
	["Data usage", "Network and Internet", "A", "ms-settings:datausage", ""],
	["Dial-up", "Network and Internet", "A", "ms-settings:network-dialup", ""],
	["Direct access", "Network and Internet", "A", "ms-settings:network-directaccess", ""],
	[
		"Ethernet",
		"Network and Internet",
		"A",
		"ms-settings:network-ethernet",
		"dns sdns securedns gateway dhcp ip"
	],
	[
		"Manage known networks",
		"Network and Internet",
		"A",
		"ms-settings:network-wifisettings",
		"wi-fi settings wifi"
	],
	["Mobile hotspot", "Network and Internet", "A", "ms-settings:network-mobilehotspot", ""],
	["NFC", "Network and Internet", "A", "ms-settings:nfctransactions", "nfc transactions"],
	["Proxy", "Network and Internet", "A", "ms-settings:network-proxy", ""],
	["Network status", "Network and Internet", "A", "ms-settings:network-status", ""],
	[
		"Network",
		"Network and Internet",
		"A",
		"ms-settings:network",
		"dns sdns securedns gateway dhcp ip"
	],
	["VPN", "Network and Internet", "A", "ms-settings:network-vpn", ""],
	[
		"Wi-Fi",
		"Network and Internet",
		"A",
		"ms-settings:network-wifi",
		"wireless metered connection wifi"
	],
	["Wi-Fi Calling", "Network and Internet", "A", "ms-settings:network-wificalling", "wifi"],
	[
		"Background",
		"Personalization",
		"A",
		"ms-settings:personalization-background",
		"wallpaper picture image"
	],
	[
		"Choose which folders appear on Start",
		"Personalization",
		"A",
		"ms-settings:personalization-start-places",
		"start places"
	],
	[
		"Colors",
		"Personalization",
		"A",
		"ms-settings:colors",
		"dark mode light mode dark color light color app color taskbar color window border"
	],
	["Lock screen", "Personalization", "A", "ms-settings:lockscreen", "image picture"],
	["Personalization (category)", "Personalization", "A", "ms-settings:personalization", ""],
	["Start", "Personalization", "A", "ms-settings:personalization-start", ""],
	["Taskbar", "Personalization", "A", "ms-settings:taskbar", ""],
	["Themes", "Personalization", "A", "ms-settings:themes", ""],
	["Add your phone", "Phone", "A", "ms-settings:mobile-devices-addphone", ""],
	["Direct open your phone", "Phone", "A", "ms-settings:mobile-devices-addphone-direct", ""],
	["Account info", "Privacy", "A", "ms-settings:privacy-accountinfo", ""],
	["Activity history", "Privacy", "A", "ms-settings:privacy-activityhistory", ""],
	["App diagnostics", "Privacy", "A", "ms-settings:privacy-appdiagnostics", ""],
	["Automatic file downloads", "Privacy", "A", "ms-settings:privacy-automaticfiledownloads", ""],
	["Background Apps", "Privacy", "A", "ms-settings:privacy-backgroundapps", ""],
	["Calendar", "Privacy", "A", "ms-settings:privacy-calendar", ""],
	["Call history", "Privacy", "A", "ms-settings:privacy-callhistory", ""],
	["Camera", "Privacy", "A", "ms-settings:privacy-webcam", ""],
	["Contacts", "Privacy", "A", "ms-settings:privacy-contacts", ""],
	["Documents", "Privacy", "A", "ms-settings:privacy-documents", ""],
	["Email", "Privacy", "A", "ms-settings:privacy-email", ""],
	["Eye tracker", "Privacy", "A", "ms-settings:privacy-eyetracker", ""],
	["Feedback and diagnostics", "Privacy", "A", "ms-settings:privacy-feedback", ""],
	["File system", "Privacy", "A", "ms-settings:privacy-broadfilesystemaccess", ""],
	["General", "Privacy", "A", "ms-settings:privacy or privacy-general", ""],
	["Inking and typing", "Privacy", "A", "ms-settings:privacy-speechtyping", "speech typing"],
	["Location", "Privacy", "A", "ms-settings:privacy-location", ""],
	["Messaging", "Privacy", "A", "ms-settings:privacy-messaging", ""],
	["Microphone", "Privacy", "A", "ms-settings:privacy-microphone", ""],
	["Motion", "Privacy", "A", "ms-settings:privacy-motion", ""],
	["Notifications", "Privacy", "A", "ms-settings:privacy-notifications", ""],
	["Other devices", "Privacy", "A", "ms-settings:privacy-customdevices", "custom devices"],
	["Phone calls", "Privacy", "A", "ms-settings:privacy-phonecalls", ""],
	["Pictures", "Privacy", "A", "ms-settings:privacy-pictures", ""],
	["Radios", "Privacy", "A", "ms-settings:privacy-radios", ""],
	["Speech", "Privacy", "A", "ms-settings:privacy-speech", ""],
	["Tasks", "Privacy", "A", "ms-settings:privacy-tasks", ""],
	["Videos", "Privacy", "A", "ms-settings:privacy-videos", ""],
	["Voice activation", "Privacy", "A", "ms-settings:privacy-voiceactivation", ""],
	["Accounts", "SurfaceHub", "A", "ms-settings:surfacehub-accounts", ""],
	["Session cleanup", "SurfaceHub", "A", "ms-settings:surfacehub-sessioncleanup", ""],
	["Team Conferencing", "SurfaceHub", "A", "ms-settings:surfacehub-calling", "calling"],
	["Team device management", "SurfaceHub", "A", "ms-settings:surfacehub-devicemanagenent", ""],
	["Welcome screen", "SurfaceHub", "A", "ms-settings:surfacehub-welcome", ""],
	["About", "System", "A", "ms-settings:about", "ram processor os id edition version"],
	["Advanced display settings", "System", "A", "ms-settings:display-advanced", ""],
	["App volume and device preferences", "System", "A", "ms-settings:apps-volume", ""],
	["Battery Saver", "System", "A", "ms-settings:batterysaver", ""],
	["Battery Saver settings", "System", "A", "ms-settings:batterysaver-settings", ""],
	[
		"Battery use",
		"System",
		"A",
		"ms-settings:batterysaver-usagedetails",
		"battery saver usage details"
	],
	["Clipboard", "System", "A", "ms-settings:clipboard", ""],
	["Display", "System", "A", "ms-settings:display", "night light blue light warmer color red eye"],
	["Default Save Locations", "System", "A", "ms-settings:savelocations", ""],
	["Screen rotation", "System", "A", "ms-settings:screenrotation", ""],
	["Duplicating my display", "System", "A", "ms-settings:quietmomentspresentation", "presentation"],
	["During these hours", "System", "A", "ms-settings:quietmomentsscheduled", "scheduled"],
	["Encryption", "System", "A", "ms-settings:deviceencryption", ""],
	["Focus assist - Quiet hours", "System", "A", "ms-settings:quiethours", ""],
	["Focus assist - Quiet moments", "System", "A", "ms-settings:quietmomentshome", ""],
	["Graphics settings", "System", "A", "ms-settings:display-advancedgraphics", "advanced graphics"],
	["Messaging", "System", "A", "ms-settings:messaging", ""],
	[
		"Multitasking",
		"System",
		"A",
		"ms-settings:multitasking",
		"timeline tab alt+tab virtual desktops"
	],
	["Night light settings", "System", "A", "ms-settings:nightlight", ""],
	["Phone - Default apps", "System", "A", "ms-settings:phone-defaultapps", ""],
	["Projecting to this PC", "System", "A", "ms-settings:project", ""],
	["Shared experiences", "System", "A", "ms-settings:crossdevice", "crossdevice"],
	["Tablet mode", "System", "A", "ms-settings:tabletmode", ""],
	["Notifications and actions", "System", "A", "ms-settings:notifications", ""],
	["Remote Desktop", "System", "A", "ms-settings:remotedesktop", ""],
	["Power and sleep", "System", "A", "ms-settings:powersleep", ""],
	["Sound", "System", "A", "ms-settings:sound", ""],
	["Storage Sense", "System", "A", "ms-settings:storagesense", ""],
	["Storage policies", "System", "A", "ms-settings:storagepolicies", ""],
	["Date and time", "Time and language", "A", "ms-settings:dateandtime", ""],
	["Japan IME settings", "Time and language", "A", "ms-settings:regionlanguage-jpnime", "jpnime"],
	["Region", "Time and language", "A", "ms-settings:regionformatting", "region formatting"],
	["Keyboard", "Time and language", "A", "ms-settings:keyboard", ""],
	["Regional language", "Time and language", "A", "ms-settings:regionlanguage", ""],
	["Bopomofo IME", "Time and language", "A", "ms-settings:regionlanguage-bpmfime", "bpmf"],
	["Cangjie IME", "Time and language", "A", "ms-settings:regionlanguage-cangjieime", ""],
	[
		"Pinyin IME settings - domain lexicon",
		"Time and language",
		"A",
		"ms-settings:regionlanguage-chsime-pinyin-domainlexicon",
		""
	],
	[
		"Pinyin IME settings - Key configuration",
		"Time and language",
		"A",
		"ms-settings:regionlanguage-chsime-pinyin-keyconfig",
		""
	],
	[
		"Pinyin IME settings - UDP",
		"Time and language",
		"A",
		"ms-settings:regionlanguage-chsime-pinyin-udp",
		""
	],
	[
		"Wubi IME settings - UDP",
		"Time and language",
		"A",
		"ms-settings:regionlanguage-chsime-wubi-udp",
		""
	],
	["Quickime", "Time and language", "A", "ms-settings:regionlanguage-quickime", ""],
	["Pinyin IME settings", "Time and language", "A", "ms-settings:regionlanguage-chsime-pinyin", ""],
	["Speech", "Time and language", "A", "ms-settings:speech", ""],
	["Wubi IME settings", "Time and language", "A", "ms-settings:regionlanguage-chsime-wubi", ""],
	["Activation", "Update and security", "A", "ms-settings:activation", ""],
	["Backup", "Update and security", "A", "ms-settings:backup", ""],
	["Delivery Optimization", "Update and security", "A", "ms-settings:delivery-optimization", ""],
	["Find My Device", "Update and security", "A", "ms-settings:findmydevice", ""],
	["For developers", "Update and security", "A", "ms-settings:developers", ""],
	["Recovery", "Update and security", "A", "ms-settings:recovery", ""],
	["Troubleshoot", "Update and security", "A", "ms-settings:troubleshoot", ""],
	[
		"Windows Security",
		"Update and security",
		"A",
		"ms-settings:windowsdefender",
		"windows defender firewall virus core isolation security processor isolated browsing exploit protection"
	],
	["Windows Insider Program", "Update and security", "A", "ms-settings:windowsinsider", ""],
	["Windows Update", "Update and security", "A", "ms-settings:windowsupdate", ""],
	[
		"Windows Update - Check for updates",
		"Update and security",
		"A",
		"ms-settings:windowsupdate-action",
		""
	],
	[
		"Windows Update - Advanced options",
		"Update and security",
		"A",
		"ms-settings:windowsupdate-options",
		""
	],
	[
		"Windows Update - Restart options",
		"Update and security",
		"A",
		"ms-settings:windowsupdate-restartoptions",
		""
	],
	[
		"Windows Update - View update history",
		"Update and security",
		"A",
		"ms-settings:windowsupdate-history",
		""
	],
	[
		"Windows Update - View optional updates",
		"Update and security",
		"A",
		"ms-settings:windowsupdate-optionalupdates",
		""
	],
	["Workplace provisioning", "User accounts", "A", "ms-settings:workplace-provisioning", ""],
	["Provisioning", "User accounts", "A", "ms-settings:provisioning", ""],
	["Windows Anywhere", "User accounts", "A", "ms-settings:windowsanywhere", ""],
	["Accessibility Options", "Ease of access", "C", "control access.cpl", "access.cpl"],
	["Action Center", "System and Security", "C", "control /name Microsoft.ActionCenter", ""],
	["Add Hardware", "Hardware and Sound", "C", "control /name Microsoft.AddHardware", ""],
	["Add/Remove Programs", "Hardware and Sound", "C", "control appwiz.cpl", "appwiz.cpl"],
	[
		"Administrative Tools",
		"System and Security",
		"C",
		"control /name Microsoft.AdministrativeTools",
		""
	],
	["AutoPlay", "Programs", "C", "control /name Microsoft.AutoPlay", ""],
	[
		"Backup and Restore",
		"System and Security",
		"C",
		"control /name Microsoft.BackupAndRestore",
		""
	],
	["Biometric Devices", "Hardware and Sound", "C", "control /name Microsoft.BiometricDevices", ""],
	[
		"BitLocker Drive Encryption",
		"System and Security",
		"C",
		"control /name Microsoft.BitLockerDriveEncryption",
		""
	],
	["Bluetooth devices", "Hardware and Sound", "C", "control /name Microsoft.BluetoothDevices", ""],
	[
		"Color management",
		"Appearance and Personalization",
		"C",
		"control /name Microsoft.ColorManagement",
		""
	],
	[
		"Credential manager",
		"User accounts",
		"C",
		"control /name Microsoft.CredentialManager",
		"password"
	],
	["Client service for NetWare", "Programs", "C", "control nwc.cpl", "nwc.cpl"],
	["Date and time", "Clock and Region", "C", "control /name Microsoft.DateAndTime", "timedate.cpl"],
	["Default location", "Clock and Region", "C", "control /name Microsoft.DefaultLocation", ""],
	["Default programs", "Programs", "C", "control /name Microsoft.DefaultPrograms", ""],
	["Device manager", "Hardware and Sound", "C", "control /name Microsoft.DeviceManager", ""],
	[
		"Devices and printers",
		"Hardware and Sound",
		"C",
		"control /name Microsoft.DevicesAndPrinters",
		""
	],
	[
		"Ease of access center",
		"Ease of access",
		"C",
		"control /name Microsoft.EaseOfAccessCenter",
		""
	],
	[
		"Folder options",
		"Appearance and Personalization",
		"C",
		"control /name Microsoft.FolderOptions",
		""
	],
	["Fonts", "Appearance and Personalization", "C", "control /name Microsoft.Fonts", ""],
	["Game controllers", "Hardware and Sound", "C", "control /name Microsoft.GameControllers", ""],
	["Get programs", "Programs", "C", "control /name Microsoft.GetPrograms", ""],
	["Getting started", "Control Panel", "C", "control /name Microsoft.GettingStarted", ""],
	["Home group", "Network and Internet", "C", "control /name Microsoft.HomeGroup", ""],
	["Indexing options", "System and Security", "C", "control /name Microsoft.IndexingOptions", ""],
	["Infrared", "Hardware and Sound", "C", "control /name Microsoft.Infrared", ""],
	[
		"Internet options",
		"Network and Internet",
		"C",
		"control /name Microsoft.InternetOptions",
		"inetcpl.cpl"
	],
	[
		"Mail - Microsoft Exchange or Windows Messaging",
		"Network and Internet",
		"C",
		"control mlcfg32.cpl",
		"mlcfg32.cpl"
	],
	["Mouse", "Hardware and Sound", "C", "control /name Microsoft.Mouse", ""],
	[
		"Network and sharing center",
		"Network and Internet",
		"C",
		"control /name Microsoft.NetworkAndSharingCenter",
		""
	],
	["Network connection", "Network and Internet", "C", "control netconnections", ""],
	["Network Setup Wizard", "Network and Internet", "C", "control netsetup.cpl", "netsetup.cpl"],
	[
		"ODBC Data Source Administrator (32-bit)",
		"Administrative Tools",
		"C",
		"%windir%/syswow64/odbcad32.exe",
		"odbccp32.cpl"
	],
	[
		"ODBC Data Source Administrator (64-bit)",
		"Administrative Tools",
		"C",
		"%windir%/system32/odbcad32.exe",
		""
	],
	["Offline files", "Network and Internet", "C", "control /name Microsoft.OfflineFiles", ""],
	["Parental controls", "User accounts", "C", "control /name Microsoft.ParentalControls", ""],
	[
		"Pen and input devices",
		"Hardware and Sound",
		"C",
		"control /name Microsoft.PenAndInputDevices",
		""
	],
	["Pen and touch", "Hardware and Sound", "C", "control /name Microsoft.PenAndTouch", ""],
	["People Near Me", "User accounts", "C", "control /name Microsoft.PeopleNearMe", ""],
	[
		"Performance information and tools",
		"System and Security",
		"C",
		"control /name Microsoft.PerformanceInformationAndTools",
		""
	],
	[
		"Phone and modem - Options",
		"Network and Internet",
		"C",
		"control /name Microsoft.PhoneAndModemOptions",
		""
	],
	[
		"Phone and modem",
		"Network and Internet",
		"C",
		"control /name Microsoft.PhoneAndModem",
		"modem.cpl"
	],
	[
		"Power options",
		"System and Security",
		"C",
		"control /name Microsoft.PowerOptions",
		"powercfg.cpl"
	],
	["Printers", "Hardware and Sound", "C", "control /name Microsoft.Printers", ""],
	[
		"Problem reports and solutions",
		"System and Security",
		"C",
		"control /name Microsoft.ProblemReportsAndSolutions",
		""
	],
	["Programs and features", "Programs", "C", "control /name Microsoft.ProgramsAndFeatures", ""],
	["Recovery", "System and Security", "C", "control /name Microsoft.Recovery", ""],
	["Region and language", "Clock and Region", "C", "control /name Microsoft.RegionAndLanguage", ""],
	[
		"RemoteApp and desktop connections",
		"Network and Internet",
		"C",
		"control /name Microsoft.RemoteAppAndDesktopConnections",
		""
	],
	[
		"Scanners and cameras",
		"Hardware and Sound",
		"C",
		"control /name Microsoft.ScannersAndCameras",
		"sticpl.cpl"
	],
	["Scheduled tasks", "System and Security", "C", "control schedtasks", "schedtasks"],
	["Security Center", "System and Security", "C", "control /name Microsoft.SecurityCenter", ""],
	["Sound", "Hardware and Sound", "C", "control /name Microsoft.Sound", ""],
	["Speech recognition", "Ease of access", "C", "control /name Microsoft.SpeechRecognition", ""],
	["Sync center", "System and Security", "C", "control /name Microsoft.SyncCenter", ""],
	["System", "System and Security", "C", "control sysdm.cpl", "sysdm.cpl"],
	[
		"Tablet PC settings",
		"System and Security",
		"C",
		"control /name Microsoft.TabletPCSettings",
		""
	],
	["Text to speech", "Ease of access", "C", "control /name Microsoft.TextToSpeech", ""],
	["User accounts", "User accounts", "C", "control /name Microsoft.UserAccounts", ""],
	["Welcome center", "System and Security", "C", "control /name Microsoft.WelcomeCenter", ""],
	[
		"Windows Anytime Upgrade",
		"System and Security",
		"C",
		"control /name Microsoft.WindowsAnytimeUpgrade",
		""
	],
	["Windows CardSpace", "Hardware and Sound", "C", "control /name Microsoft.CardSpace", ""],
	["Windows Defender", "System and Security", "C", "control /name Microsoft.WindowsDefender", ""],
	["Windows Firewall", "System and Security", "C", "control /name Microsoft.WindowsFirewall", ""],
	[
		"Windows Mobility Center",
		"Network and Internet",
		"C",
		"control /name Microsoft.MobilityCenter",
		""
	],
	["Display properties", "Hardware and Sound", "C", "control Desk.cpl", "desk.cpl"],
	["FindFast", "System and Security", "C", "control FindFast.cpl", "findfast.cpl"],
	["Regional settings properties", "Ease of access", "C", "control Intl.cpl", "intl.cpl"],
	["Joystick properties", "Hardware and Sound", "C", "control Joy.cpl", "joy.cpl"],
	[
		"Mouse, Fonts, Keyboard, and Printers properties",
		"Hardware and Sound",
		"C",
		"control Main.cpl",
		"main.cpl"
	],
	["Multimedia properties", "Hardware and Sound", "C", "control Mmsys.cpl", "mmsys.cpl"],
	["Network properties", "Network and Internet", "C", "control Netcpl.cpl", "netcpl.cpl"],
	["Password properties", "User accounts", "C", "control Password.cpl", "password.cpl"],
	[
		"System properties and Add New Hardware wizard",
		"Hardware and Sound",
		"C",
		"control Sysdm.cpl",
		"sysdm.cpl"
	],
	["Desktop themes", "Appearance and Personalization", "C", "control Themes.cpl", "themes.cpl"],
	["Microsoft Mail Post Office", "Programs", "C", "control Wgpocpl.cpl", "wgpocpl.cpl"],
	[
		"Accommodate learning abilities",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageQuestionsCognitive",
		"accomidate accomodates accomodating adjust for can not cannot can't difficult to difficulty does doesn't hard will willnot won't disabled handicap handicapped disabilities disability empaired impaired imparied trouble cognitive learning"
	],
	[
		"Accommodate low vision",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"accomidate accomodates accomodating adjust for site blindness blurred blurry half legally low vision poor seeing seems sight weak bad eyesight eye-sight disabled handicap handicapped disabilities disability empaired impaired imparied trouble best better easier easy enhance improvement make more most optimal optimise optimize"
	],
	[
		"Change how your keyboard works",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageKeyboardEasierToUse",
		"switch filterkeys keys stickykeys togglekeys sticky-keys toggle-keys repeat delay hold down rate accessibility acessibility ease of best better easier easy enhance improvement make more most optimal optimise optimize board bord keyboards keyborads keybord"
	],
	[
		"Change how your mouse works",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToClick",
		"accessibility acessibility ease of butons buttons adjust ajust alter change edit modify replace reset set switch lock clicking doubleclick double-click single-click best better easier easy enhance improvement make more most optimal optimise optimize move moving shadow speed tails mice mouses pointer"
	],
	[
		"Use screen reader",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageNoVisual",
		"adjust ajust alter change edit modify replace reset set switch screan readers screenreader windows narrator"
	],
	[
		"Change the Narrator’s voice",
		"",
		"T",
		"%windir%\\system32\\narrator.exe",
		"adjust ajust alter change edit modify replace reset set switch sound voices screan readers screenreader windows narrator"
	],
	[
		"Control the computer without the mouse or keyboard",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageNoMouseOrKeyboard",
		"laptop compuer machine my computer pc personal system board bord keyboards keyborads keybord mice mouses pointer recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice"
	],
	[
		"Hear a tone when keys are pressed",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageFilterKeysSettings",
		"def hearing deaf deafness hard of problem impaired imparied audio beeps music noises schemes sounds tones wav sound beep noise customise soundsentry sentry settings customize captions showsounds"
	],
	[
		"Hear text read aloud with Narrator",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter",
		"read reads reading screan readers screenreader windows narrator"
	],
	[
		"Ignore repeated keystrokes using FilterKeys",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageFilterKeysSettings",
		"keys filterkeys"
	],
	[
		"Let Windows suggest Ease of Access settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageQuestionsEyesight",
		"accessibility acessibility ease of laptop compuer machine my computer pc personal system best better easier easy enhance improvement make more most optimal optimise optimize questionaire questionnaire survey wizard wizzard"
	],
	[
		"Magnify portions of the screen using Magnifier",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"site blindness blurred blurry half legally low vision poor seeing seems sight weak bad eyesight eye-sight adjust ajust alter change edit modify replace reset set switch magnification magnified magnifying make bigger larger zoom in crt displays monitors screeens screens"
	],
	[
		"Move the pointer with the keypad using MouseKeys",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageKeyboardEasierToUse",
		"arrows cursors mouse pointers mice mouses pointer keys mousekeys"
	],
	[
		"Optimise for blindness",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageNoVisual",
		"site blindness blurred blurry half legally low vision poor seeing seems sight weak bad eyesight eye-sight can not cannot can't difficult to difficulty does doesn't hard will willnot won't customising customisation customises customization customizes customizing personalisation personalise personalization personalize dimmed dimed dimming diming dimmer dimer display fade brightness disabled handicap handicapped disabilities disability empaired impaired imparied trouble best better easier easy enhance improvement make more most optimal optimise optimize set-up confg configuration configure define management options up settings setup"
	],
	[
		"Optimise visual display",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"switch filterkeys keys stickykeys togglekeys sticky-keys toggle-keys repeat delay hold down rate appearance out layout of looks way displayed site blindness blurred blurry half legally low vision poor seeing seems sight weak bad eyesight eye-sight adjust ajust alter change edit modify replace reset set customising customisation customises customization customizes customizing personalisation personalise personalization personalize best better easier easy enhance improvement make more most optimal optimise optimize set-up confg configuration configure define management options up settings setup"
	],
	[
		"Press key combinations one at a time",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageKeyboardEasierToUse",
		"board bord keyboards keyborads keybord keys stickykeys"
	],
	[
		"Replace sounds with visual cues",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierWithSounds",
		"can not cannot can't difficult to difficulty does doesn't hard will willnot won't def hearing deaf deafness of problem impaired imparied disabled handicap handicapped disabilities disability empaired trouble cards soundcards audio beeps music noises schemes sounds tones wav sound beep noise customise soundsentry sentry settings customize captions showsounds"
	],
	[
		"Turn High Contrast on or off",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL desk.cpl,,2",
		"brightness contrast hi-contrast high-contrast activate add begin enable intsall invoke make up setting setup start on turnon unlock monitors screens deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff crt displays screeens"
	],
	[
		"Turn Magnifier on or off",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock magnification magnified magnifying bigger larger zoom in"
	],
	[
		"Turn off background images",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"ground background image photograph picture deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Turn off unnecessary animations",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"animations cartoons deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Turn On-Screen keyboard on or off",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock board bord keyboards keyborads keybord screen keyboard onscreen on-screen osk"
	],
	[
		"Turn on easy access keys",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageKeyboardEasierToUse",
		"board bord keyboards keyborads keybord"
	],
	[
		"Use audio description for video",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToSee",
		"text to speech tts voice narrator videoes videos"
	],
	[
		"View current accessibility settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter",
		"accessibility acessibility ease of set-up confg configuration configure define management options personalise personalize up settings setup check list see show viewing"
	],
	[
		"Turn off automatic window arrangement",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.EaseOfAccessCenter /page pageEasierToClick",
		"mice mouses pointer management managment movement movment movemnet motion maximise minimise snapping shake shaking dock drag edge sides top screen size resize maximize restore minimize"
	],
	[
		"Create and format hard disk partitions",
		"",
		"T",
		"%windir%\\system32\\diskmgmt.msc",
		"clean-up check disk dsk chkdsk cleandisk cleaner cleanup up add create make new disc disks drive activate begin enable intsall invoke setting setup start on turnon unlock fdisc fdisk formated formating formatted formatting harddisk hard-disk harddrive hard-drive boot extended format ntfs fat partitian partitioning primary reformat sector unpartition volume list see show viewing management managment"
	],
	[
		"Defragment and optimise your drives",
		"",
		"T",
		"%windir%\\system32\\dfrgui.exe",
		"clean-up check disk dsk chkdsk cleandisk cleaner cleanup up defragment free space disc disks drive harddisk hard-disk harddrive hard-drive scandisc scandisk scandsk list see show viewing"
	],
	[
		"Diagnose your computer's memory problems",
		"",
		"T",
		"%windir%\\system32\\mdsched.exe",
		"diagnose diagnostics diagnosis find problems analyse analyze analysis troubleshooter errors gigabytes gigs megabytes megs memory ram"
	],
	[
		"Edit group policy",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\gpedit.msc",
		"adjust ajust alter change edit modify replace reset set switch grouping policies policys"
	],
	[
		"Clear disk space by deleting unnecessary files",
		"",
		"T",
		"%windir%\\system32\\cleanmgr.exe",
		"clean-up check disk dsk chkdsk cleandisk cleaner cleanup up free space room release available drive harddisk hard-disk harddrive hard-drive deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff temp cached files history local temorary temporary temperary tmp unnecessary unneeded useless web"
	],
	[
		"Generate a system health report",
		"",
		"T",
		"%windir%\\system32\\perfmon.exe /report",
		"laptop compuer machine my computer pc personal system healthy optimise crawl degrade faster frozen hanging hung improve inactive optimize performance quicker sluggish speed thrashing too slow unresponsive diagnose diagnostics diagnosis find problems analyse analyze analysis troubleshooter errors"
	],
	[
		"Schedule tasks",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\taskschd.msc",
		"automated automatically scheduled scheduling"
	],
	[
		"Set up ODBC data sources",
		"",
		"T",
		"%windir%\\system32\\odbcad32.exe",
		"datum odbc data-base database activate add begin enable intsall invoke make up setting setup start on turnon unlock"
	],
	[
		"Set up ODBC data sources (32-bit)",
		"",
		"T",
		"%windir%\\syswow64\\odbcad32.exe",
		"datum odbc data-base database activate add begin enable intsall invoke make up setting setup start on turnon unlock"
	],
	[
		"Set up ODBC data sources (64-bit)",
		"",
		"T",
		"%windir%\\system32\\odbcad32.exe",
		"datum odbc data-base database activate add begin enable intsall invoke make up setting setup start on turnon unlock"
	],
	[
		"Set up iSCSI initiator",
		"",
		"T",
		"%windir%\\system32\\iscsicpl.exe",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock devices divises divicse divices iscsi scsi storage"
	],
	[
		"View event logs",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\eventvwr.msc",
		"viewer events tracker eventviewer check list see show viewing"
	],
	[
		"View local services",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\services.msc",
		"disable stop services activate add begin enable intsall invoke make up setting setup start on turnon unlock"
	],
	[
		"Manage computer certificates",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\certlm.msc",
		"certificate key"
	],
	[
		"Uninstall a program",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ProgramsAndFeatures",
		"file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows browsers ie microsoft deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff deinstall uninstalled uninstalling uninstalls unistall"
	],
	[
		"Add or remove programs",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ProgramsAndFeatures",
		"add or remove programs and arp"
	],
	[
		"View installed updates",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ProgramsAndFeatures /page ::{D450A8A1-9568-45C7-9C0E-B4F9FB4537BD}",
		"previous old installed already previously atuo automatically automaticupdates autoupdate fixes bugfixes bugs download live microsoft patches scan security service packs dater dates dating updater up-dater updater.exe updates up-dates up-dating windows windowsupdate.com wizard wupdater notifications notifying notified"
	],
	[
		"Turn Windows features on or off",
		"",
		"T",
		"%windir%\\system32\\OptionalFeatures.exe",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock accessories accessory components feature features programs tools utilities utility"
	],
	[
		"Show which programs are installed on your computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ProgramsAndFeatures",
		"laptop compuer machine my computer pc personal system file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows automatically autoshow auto-show make visible see show unlock view"
	],
	[
		"How to install a program",
		"",
		"T",
		"mshelp://windows/?id=fe7ea80e-52a2-48d6-947a-05e02e78bc37",
		"added adding addon add-on adds activate add begin enable intsall invoke make up setting setup start on turnon unlock file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows browsers ie microsoft"
	],
	[
		"Install a program from the network",
		"",
		"T",
		"shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}\\8\\::{15EAE92E-F17A-4431-9F28-805E482dAFD4}",
		"add or remove programs and arp new installed installing file applets applications apps aps files programmes softare softwares sofware sotfware windos windows"
	],
	[
		"Change or remove a program",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ProgramsAndFeatures",
		"adjust ajust alter change edit modify replace reset set switch file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows browsers ie microsoft deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff deinstall uninstalled uninstalling uninstalls unistall"
	],
	[
		"16-Bit Application Support",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL ntvdmcpl.dll",
		"file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows application app apcompat appcompat compatable compatibility compatible still use 16 bit 16-bit legacy old dos win31 emulation ntvdm"
	],
	[
		"Run programs made for previous versions of Windows",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id PCWDiagnostic -ep ControlPanelSearch",
		"applications apps application app apcompat appcompat compatable compatibility compatible still use earlier older previous old installed already previously browsers ie microsoft windows file applets aps files programmes programs softare softwares sofware sotfware windos"
	],
	[
		"Make a file type always open in a specific program",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DefaultPrograms /page pageFileAssoc",
		"adjust ajust alter change edit modify replace reset set switch associate association associations extensions extentions type filetype open with file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows"
	],
	[
		"Set your default programs",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DefaultPrograms /page pageDefaultProgram",
		"adjust ajust alter change edit modify replace reset set switch by defaults defualt standard associate association associations extensions extentions type filetype open with file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows protocols"
	],
	[
		"View devices and printers",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DevicesAndPrinters",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock connections go online intranet lan netowrk networking line on-line www 802.1x wireless wire-less out printers printing printner printout print-out pritner computer top laptop mobile pc notebook portable devices divises divicse divices 1394 controller adapter adaptor audio biometric cable modem card cd-dvd drive crt devise digial camera dsl dvd ethernet flat panel game handheld scanner ware hardware hardwear infrared ink-jet input irda stick joystick keyboard laser printer lcd mass storage midi mixer monitor motherboard mouse mp3 player mpeg music network optical wheel output pen plasma media raid recording reader smartcard sound tape pad touchpad ball trackball tuner tv usb video capture cards view hub webcam wlan added adding addon add-on adds scaners scanners scanning camaras cameras camras maneger manager devicemanager dma managar management managing managment drivers blank ray blue-ray blu-ray cd-rom cds enhanced hd super tooth bluetooth blutooth videocards videos hardware-general"
	],
	[
		"Add a device",
		"",
		"T",
		"%windir%\\system32\\DevicePairingWizard.exe",
		"devices divises divicse divices mice mouses pointer board bord keyboards keyborads keybord out printers printing printner printout print-out pritner added adding addon add-on adds add new installed installing mobile handset handsets phone cellphone cell speakers headset headsets tv television dlna media player storage nas network networking networks link pair pairs associate associates connects connect setup playto play"
	],
	[
		"Advanced printer setup",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe printui.dll,PrintUIEntryDPIAware /il",
		"out printers printing printner printout print-out pritner added adding addon add-on adds add new installed installing finding locate locating search for is where's the gets getting get-verb network networking networks link pair pairs associate associates connects connect setup playto play"
	],
	[
		"Add a Bluetooth device",
		"",
		"T",
		"%windir%\\system32\\DevicePairingWizard.exe /bluetooth",
		"tooth bluetooth blutooth devices divises divicse divices 802.1x wireless wire-less link pair pairs associate associates add adds connects connect setup playto play new installed installing"
	],
	[
		"Device manager",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DeviceManager",
		"maneger manager devicemanager dma managar management managing managment devices divises divicse divices drivers activate add begin enable intsall invoke make up setting setup start on turnon unlock 1394 controller adapter adaptor audio biometric cable modem card cd-dvd drive crt devise digial camera dsl dvd ethernet flat panel game handheld scanner ware hardware hardwear infrared ink-jet input irda stick joystick keyboard laser printer lcd mass storage midi mixer mobile monitor motherboard mouse mp3 player mpeg music network optical wheel output pen plasma portable media raid recording reader smartcard sound tape pad touchpad ball trackball tuner tv usb video capture cards view hub webcam wireless wlan mine my"
	],
	[
		"Change Windows To Go start-up options",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe pwlauncher.dll,ShowPortableWorkspaceLauncherConfigurationUX",
		""
	],
	[
		"Change default printer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DevicesAndPrinters",
		"adjust ajust alter change edit modify replace reset set switch by defaults defualt standard finding locate locating search for is where's the gets getting get-verb out printers printing printner printout print-out pritner"
	],
	[
		"View scanners and cameras",
		"",
		"T",
		"%ProgramFiles%\\Windows Photo Viewer\\ImagingDevices.exe",
		"camaras cameras camras automatically autoshow auto-show make visible see show unlock view check list viewing"
	],
	[
		"Change Bluetooth settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL bthprops.cpl,,1",
		"tooth bluetooth blutooth adjust ajust alter change edit modify replace reset set switch devices divises divicse divices 802.1x wireless wire-less"
	],
	[
		"Scan a document or picture",
		"",
		"T",
		"%windir%\\system32\\wfs.exe",
		"scaners scanners scanning add create make new"
	],
	[
		"Set up USB game controllers",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL %windir%\\system32\\joy.cpl",
		"adjust ajust alter change edit modify replace reset set switch controlers controllers stick joystick devices divises divicse divices games"
	],
	[
		"Change device installation settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe newdev.dll,DeviceInternetSettingUi 5",
		"change changing devices installation installing settings hardware drivers windows in out optin opt-in optout opt-out metadata data icons stage downloads downloading online fixing working my searching finding getting doesn't not for newer notifications notifying notified"
	],
	[
		"Change settings for content received using Tap and send",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.AutoPlay",
		"nfp near field feild proximity prosimity proxymity tap and send"
	],
	[
		"Change advanced colour management settings for displays, scanners and printers",
		"",
		"T",
		"%windir%\\system32\\colorcpl.exe",
		"calibrate calibration 24 bit 24-bit 256 32-bit colors colours depth num number of absolute colorimetric gamut mapping graphics images perceptual proofing relative rendering rgb srgb management managment out printers printing printner printout print-out pritner scaners scanners scanning crt displays monitors screeens screens"
	],
	[
		"Add clocks for different time zones",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL timedate.cpl,,1",
		"additional another create many more mutliple adjust ajust alter change edit modify replace reset set switch days calendar clock times time day time zones timezone time-zone zones set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Automatically adjust for daylight saving time",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL timedate.cpl",
		"automated automatically adjust ajust alter change edit modify replace reset set switch days calendar clock times time day light daylight daylite lite savings savins standard ddmmyyyy hour 12-hour 24-hour date dd display formating formatting four digit long mmddyyyy month properties short two year yyyy"
	],
	[
		"Change the time zone",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL timedate.cpl",
		"adjust ajust alter change edit modify replace reset set switch time zones timezone time-zone zones"
	],
	[
		"Set the time and date",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL timedate.cpl",
		"adjust ajust alter change edit modify replace reset set switch days calendar clock times time day ddmmyyyy hour 12-hour 24-hour date dd display formating formatting four digit long mmddyyyy month properties short two year yyyy"
	],
	[
		"Change default settings for media or devices",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.AutoPlay",
		"play run start autoplay auto-play autorun auto-run autostart auto-start audio blank ray blue-ray blu-ray cd-rom cds dvd enhanced hd media super video import photo connection camera flash removable storage stage"
	],
	[
		"Play CDs or other media automatically",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.AutoPlay",
		"automated automatically play run start autoplay auto-play autorun auto-run autostart auto-start disc disks drive audio blank ray blue-ray blu-ray cd-rom cds dvd enhanced hd media super video"
	],
	[
		"Start or stop using AutoPlay for all media and devices",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.AutoPlay",
		"play run start autoplay auto-play autorun auto-run autostart auto-start activate add begin enable intsall invoke make up setting setup on turnon unlock deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Change search options for files and folders",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Options_RunDLL 2",
		"folders subfolders behaviour search behavior options finder indexing reindex re-index searches searching"
	],
	[
		"Change the file type associated with a file extension",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.DefaultPrograms /page pageFileAssoc",
		"adjust ajust alter change edit modify replace reset set switch associate association associations extensions extentions type filetype open with file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows"
	],
	[
		"Show hidden files and folders",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Options_RunDLL 7",
		"folders subfolders automatically autoshow auto-show make visible see show unlock view"
	],
	[
		"Show or hide file extensions",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Options_RunDLL 7",
		"associate association associations extensions extentions type filetype open with autohide auto-hide automatically hide make invisible autoshow auto-show visible see show unlock view"
	],
	[
		"Specify single- or double-click to open",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.FolderOptions",
		"choice chooses choosing chose picked picks selected selects lock clicking doubleclick double-click single-click"
	],
	[
		"Preview, delete, show or hide fonts",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Fonts",
		"faunt fonts type truetype opentype fount added adding addon add-on adds add new installed installing deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff deinstall uninstalled uninstalling uninstalls unistall"
	],
	[
		"Change Font Settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Fonts /page ::{93412589-74D4-4E4E-AD0E-E0CB621440FD}",
		"faunt fonts type truetype opentype fount set-up confg configuration configure define management options personalise personalize up settings setup autohide auto-hide automatically hide make invisible autoshow auto-show visible see show unlock view"
	],
	[
		"View installed fonts",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Fonts",
		"faunt fonts type truetype opentype fount add new installed installing check list see show viewing"
	],
	[
		"Private Character Editor",
		"",
		"T",
		"%windir%\\system32\\eudcedit.exe",
		"eudcedit.exe private character editor"
	],
	[
		"Adjust commonly used mobility settings",
		"",
		"T",
		"%windir%\\system32\\mblctr.exe /open",
		"adjust ajust alter change edit modify replace reset set switch computer top laptop mobile pc notebook portable set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Adjust settings before giving a presentation",
		"",
		"T",
		"%windir%\\system32\\presentationsettings.exe",
		"adjust ajust alter change edit modify replace reset set switch pixels reslution resoltion resolution resolutoin rezolution size crt displays monitors screeens screens set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Block or allow pop-ups",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,2",
		"banned blocker filter nanny prevent restrict stopper from the web inet internet intrnet net online on-line sites pages webpages websites world wide www ads advertising advertizing adverts adware adwear malware ups popupblocker pop-upblocker popups pop-ups popupstopper spyware spywear unwanted windows private privacy secrecy restricted safety secure securing security trusted trusting set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Block or allow third-party cookies",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,2",
		"allowed let through permitted banned blocker filter nanny prevent restrict stopper 3rd party cookies cooky from others third-party unwanted the web inet internet intrnet net online on-line sites pages webpages websites world wide www restricted safety secure securing security trusted trusting"
	],
	[
		"Change how web pages are displayed in tabs",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"browsers browsing web ie internet explorer from the inet intrnet net online on-line sites pages webpages websites world wide www tabs tabbed quick"
	],
	[
		"Change security settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,1",
		"allowed let through permitted banned blocker filter nanny prevent restrict stopper adjust ajust alter change edit modify replace reset set switch from the web inet internet intrnet net online on-line sites pages webpages websites world wide www restricted safety secure securing security trusted trusting"
	],
	[
		"Change temporary Internet file settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"adjust ajust alter change edit modify replace reset set switch 3rd party cookies cooky from others third-party unwanted the web inet internet intrnet net online on-line sites pages webpages websites world wide www deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff set-up confg configuration configure define management options personalise personalize up settings setup temp cached files history local temorary temporary temperary tmp unnecessary unneeded useless"
	],
	[
		"Change the search provider in Internet Explorer",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"adjust ajust alter change edit modify replace reset set switch by defaults defualt standard provider engine searching"
	],
	[
		"Change your homepage",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"adjust ajust alter change edit modify replace reset set switch page homepage from the web inet internet intrnet net online on-line sites pages webpages websites world wide www"
	],
	[
		"Configure proxy server",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,4",
		"server web winsock socks socket dns sas winproxy rpoxy client hosts tmproxy proxycfg set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Connect to the Internet",
		"",
		"T",
		"ms-availablenetworks:",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock from the web inet internet intrnet net online on-line sites pages webpages websites world wide www connections go intranet lan netowrk networking line wizard wizzard"
	],
	[
		"Delete browsing history",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"browsers browsing web autohide auto-hide automatically hide make invisible ie internet explorer from the inet intrnet net online on-line sites pages webpages websites world wide www deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff temp cached files history local temorary temporary temperary tmp unnecessary unneeded useless"
	],
	[
		"Delete cookies or temporary files",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,0",
		"3rd party cookies cooky from others third-party unwanted deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff temp cached files history local temorary temporary temperary tmp unnecessary unneeded useless web"
	],
	[
		"Enable or disable session cookies",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,2",
		"3rd party cookies cooky from others third-party unwanted activate add begin enable intsall invoke make up setting setup start on turnon unlock the web inet internet intrnet net online on-line sites pages webpages websites world wide www deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff restricted safety secure securing security trusted trusting"
	],
	[
		"Manage browser add-ons",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,5",
		"activex ons add-ons extension browsers browsing web add new installed installing administer configure managed manages managing up setup deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff deinstall uninstalled uninstalling uninstalls unistall"
	],
	[
		"Tell if an RSS feed is available on a website",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,3",
		"from the web inet internet intrnet net online on-line sites pages webpages websites world wide www rss feed aggregator csrss"
	],
	[
		"Turn autocomplete in Internet Explorer on or off",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,3",
		"autocomplete complete completion finish autofinish fill autofill activate add begin enable intsall invoke make up setting setup start on turnon unlock deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Choose how you open links",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL inetcpl.cpl,,5",
		"desktops dekstops lanch launches ie internet explorer browsers browsing web opens opening opened links tiles always in let decide"
	],
	[
		"Change cursor blink rate",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Keyboard",
		"blinking blinkrate flashing rate arrows cursors mouse pointers board bord keyboards keyborads keybord"
	],
	[
		"Check keyboard status",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Keyboard /page 1",
		"board bord keyboards keyborads keybord"
	],
	[
		"Change mouse settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse",
		"butons buttons adjust ajust alter change edit modify replace reset set switch lock clicking doubleclick double-click single-click mice mouses pointer flip handed hander left-handed left-hander lefty reverse right-handed set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Change how the mouse pointer looks",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse /page 1",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch arrows cursors mouse pointers mice mouses pointer"
	],
	[
		"Change how the mouse pointer looks when it’s moving",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse /page 2",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch arrows cursors mouse pointers move moving shadow speed tails mice mouses pointer"
	],
	[
		"Change mouse click settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse",
		"adjust ajust alter change edit modify replace reset set switch lock clicking doubleclick double-click single-click mice mouses pointer set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Change mouse wheel settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse /page 3",
		"adjust ajust alter change edit modify replace reset set switch mice mouses pointer set-up confg configuration configure define management options personalise personalize up settings setup wheel scrolling scrollwheel mousewheel"
	],
	[
		"Change the mouse pointer display or speed",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse /page 2",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch arrows cursors mouse pointers move moving shadow speed tails mice mouses pointer"
	],
	[
		"Customise the mouse buttons",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse",
		"butons buttons adjust ajust alter change edit modify replace reset set switch customising customisation customises customization customizes customizing personalisation personalise personalization personalize mice mouses pointer flip handed hander left-handed left-hander lefty reverse right-handed set-up confg configuration configure define management options up settings setup"
	],
	[
		"Make it easier to see the mouse pointer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Mouse /page 2",
		"arrows cursors mouse pointers finding locate locating search for is where's the cannot cant find can't see gone hidden invisible missing vanished where mice mouses pointer"
	],
	[
		"Get more features with a new edition of Windows",
		"",
		"T",
		"%SystemRoot%\\system32\\WindowsAnytimeUpgradeUI.exe",
		"compare comparison side-by-side features get more next version vista home business premium ultimate starter windows windw 7 release edition centre anytime upgrade media center play dvd watch tv television record"
	],
	[
		"Set up dialling rules",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PhoneAndModemOptions",
		"calling calls area code carrier city up dialing dialling dialup dial-up pulse tone touchtone touch-tone modem phone line telephone rules"
	],
	[
		"Change battery settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions",
		"adjust ajust alter change edit modify replace reset set switch batteries battery life conserve energy last longer management plugged in supply powersaver preserve save mode saving unplugged ups set-up confg configuration configure define options personalise personalize up settings setup"
	],
	[
		"Change power-saving settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions",
		"adjust ajust alter change edit modify replace reset set switch customising customisation customises customization customizes customizing personalisation personalise personalization personalize batteries battery life conserve energy last longer management plugged in supply powersaver preserve save mode saving unplugged ups set-up confg configuration configure define options up settings setup"
	],
	[
		"Change what closing the lid does",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions /page pageGlobalSettings",
		"adjust ajust alter change edit modify replace reset set switch close closing def hearing deaf deafness hard of problem impaired imparied cover lid top batteries battery life conserve energy last longer management plugged in supply powersaver preserve save mode saving unplugged ups set-up confg configuration configure define options personalise personalize up settings setup"
	],
	[
		"Change what the power buttons do",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions /page pageGlobalSettings",
		"adjust ajust alter change edit modify replace reset set switch on-off turn hibernate button down power-button shutdown shut-down shuts by standby stand-by switches turns off batteries battery life conserve energy last longer management plugged in supply powersaver preserve save mode saving unplugged ups set-up confg configuration configure define options personalise personalize up settings setup"
	],
	[
		"Change when the computer sleeps",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions /page pagePlanSettings",
		"adjust ajust alter change edit modify replace reset set switch batteries battery life conserve energy last longer management plugged in supply powersaver preserve save mode saving unplugged ups deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff asleep autowake awaken come back from hibernates hibernating hibernation restarts resumes sleeping standby up wakeup wake-up waking"
	],
	[
		"Choose when to turn off display",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions /page pagePlanSettings",
		"adjust ajust alter change edit modify replace reset set switch turn hibernate button down power-button shutdown shut-down shuts by standby stand-by switches turns off crt displays monitors screeens screens"
	],
	[
		"Choose a power plan",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions",
		"adjust ajust alter change edit modify replace reset set switch plans choose select power scheme balanced saver crt displays monitors screeens screens"
	],
	[
		"Edit power plan",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.PowerOptions /page pagePlanSettings",
		"adjust ajust alter change edit modify replace reset set switch plans choose select power scheme balanced saver crt displays monitors screeens screens"
	],
	[
		"Change screen saver",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL desk.cpl,screensaver,@screensaver",
		"savers screan screansavers screeen screeensavers screen screen-saver screensavers scren screne screnesavers scrensavers scrren scrrensavers"
	],
	[
		"Turn screen saver on or off",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL desk.cpl,screensaver,@screensaver",
		"deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff savers screan screansavers screeen screeensavers screen screen-saver screensavers scren screne screnesavers scrensavers scrren scrrensavers"
	],
	[
		"Change date, time or number formats",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL intl.cpl",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch sterling pence cents currency dollars euros money pounds sign symbol won yen yuan show formated formating formatted formatting m centimeters centimetres cm feet foot inches kilometers kilometres kilos km lbs measurements meters metres miles numbers ounces oz weight width countries country dev region language localisation localised locality localization localized 1 2 3 code regional regions locale set-up confg configuration configure define management options personalise personalize up settings setup ddmmyyyy hour 12-hour 24-hour clock date day dd display four digit long mmddyyyy month properties short time two year yyyy alphabetise alphabetize order sorting"
	],
	[
		"Change the way currency is displayed",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL intl.cpl",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch sterling pence cents currency dollars euros money pounds sign symbol won yen yuan set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Change the way dates and lists are displayed",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL intl.cpl",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch set-up confg configuration configure define management options personalise personalize up settings setup ddmmyyyy hour 12-hour 24-hour clock date day dd display formating formatting four digit long mmddyyyy month properties short time two year yyyy"
	],
	[
		"Change the way measurements are displayed",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL intl.cpl",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch m centimeters centimetres cm feet foot inches kilometers kilometres kilos km lbs measurements meters metres miles numbers ounces oz pounds weight width set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Change the way time is displayed",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL intl.cpl",
		"appearance out layout of looks way displayed adjust ajust alter change edit modify replace reset set switch set-up confg configuration configure define management options personalise personalize up settings setup ddmmyyyy hour 12-hour 24-hour clock date day dd display formating formatting four digit long mmddyyyy month properties short time two year yyyy"
	],
	[
		"Add a language",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Language",
		"adjust ajust alter change edit modify replace reset set switch choice chooses choosing chose picked picks selected selects activate add begin enable intsall invoke make up setting setup start on turnon unlock british english foreign languages french german hindi international japanese korean langpack langs multilanguage multi-language pack spanish uk usa dialogues text words menus dialogs buttons labels writing ui language messages mesages"
	],
	[
		"Change input methods",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Language",
		"adjust ajust alter change edit modify replace reset set switch letter characters chinese hangul japanese kanji korean writing activate add begin enable intsall invoke make up setting setup start on turnon unlock input methods imes board bord keyboards keyborads keybord british english foreign languages french german hindi international langpack langs multilanguage multi-language pack spanish uk usa layout out set-up confg configuration configure define management options personalise personalize settings"
	],
	[
		"Settings for Microsoft IME (Japanese)",
		"",
		"T",
		"%windir%\\system32\\ime\\imejp\\imjpset.exe",
		"msime ime settings japanese"
	],
	[
		"Microsoft Pinyin SimpleFast Options",
		"",
		"T",
		"%windir%\\system32\\ime\\imesc\\imscprop.exe /EXPRESS",
		"msime ime settings pinyin mspy"
	],
	[
		"Microsoft New Phonetic Settings",
		"",
		"T",
		"%windir%\\system32\\ime\\imetc\\imtcprop.exe /NPH",
		"msime ime settings phonetic"
	],
	[
		"Microsoft ChangJie Settings",
		"",
		"T",
		"%windir%\\system32\\ime\\imetc\\imtcprop.exe /CJ",
		"msime ime settings changjie"
	],
	[
		"Microsoft Quick Settings",
		"",
		"T",
		"%windir%\\system32\\ime\\imetc\\imtcprop.exe /QK",
		"msime ime settings quick"
	],
	[
		"Microsoft IME Register Word (Japanese)",
		"",
		"T",
		"%windir%\\system32\\ime\\imejp\\imjpdct.exe",
		"msime ime register word japanese"
	],
	[
		"Change how Windows searches",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.IndexingOptions /page 2",
		"adjust ajust alter change edit modify replace reset set switch finder indexing reindex re-index searches searching set-up confg configuration configure define management options personalise personalize up settings setup vista windows windw 7"
	],
	[
		"View recent messages about your computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter",
		"warning report alert windows crash hang balloon reminder messages mesages notifications notifying notified"
	],
	[
		"Review your computer's status and resolve issues",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter",
		"healthy checking look for scanning verify restricted safety secure securing security sites trusted trusting condition running state status fix repair error warning alert windows crash hang troubleshoot diagnose reports diagnostic diagnosis balloon reminder performance maintenance maintain reinstall recovery restore cpc completepc pc system antivirus anti-virus spyware networking connection connectivity messages mesages notifications notifying notified download view drivers get instructions recommendations"
	],
	[
		"Change User Account Control settings",
		"",
		"T",
		"%windir%\\system32\\UserAccountControlSettings.exe",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff restricted safety secure securing security sites trusted trusting dialogues credentials elevations lua prompts prompting warnings uac administrator rights privileges permissions users accounts controls administrater dialogs popups"
	],
	[
		"View reliability history",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter /page pageReliabilityView",
		"repair problem error crash hang reliability monitor history system stability index"
	],
	[
		"Fix problems with your computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter",
		"fix repair crash hang performance maintenance maintain problems reports and re-instate re-set recovery recover reinstall factory re-image reimage re-install restore reinstate reset wipe rollback troubleshoot pc refresh erase format messages mesages"
	],
	[
		"View recommended actions to keep Windows running smoothly",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter",
		"fix repair crash hang performance maintenance maintain problems reports and messages mesages"
	],
	[
		"Check security status",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter",
		"centre security center firewall malware virus warning windows crash antivirus anti-virus spyware worm trojan"
	],
	[
		"View all problem reports",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter /page pageProblems",
		"reports tell microsoft send sent clear problems delete history and erase view all"
	],
	[
		"Change Automatic Maintenance settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.ActionCenter /page MaintenanceSettings",
		"adjust ajust alter change edit modify replace reset set switch set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Adjust system volume",
		"",
		"T",
		"%windir%\\system32\\sndvol.exe",
		"adjust ajust alter change edit modify replace reset set switch laptop compuer machine my computer pc personal system customising customisation customises customization customizes customizing personalisation personalise personalization personalize not hear can't sound volume is off too low audio beeps music noises schemes sounds tones wav beep noise card external internal speakers louder mute quiet quite soft devices divises divicse divices"
	],
	[
		"Change sound card settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Sound",
		"adjust ajust alter change edit modify replace reset set switch set-up confg configuration configure define management options personalise personalize up settings setup cards soundcards audio beeps music noises schemes sounds tones wav sound beep noise"
	],
	[
		"Change system sounds",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Sound /page 2",
		"adjust ajust alter change edit modify replace reset set switch laptop compuer machine my computer pc personal system not hear can't sound volume is off too low set-up confg configuration configure define management options personalise personalize up settings setup audio beeps music noises schemes sounds tones wav beep noise"
	],
	[
		"Manage audio devices",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Sound",
		"audio balance card effect encoding jack level line microphone mono multichannel multi-channel port sound speaker stereo subwoofer surround in line-in adjust ajust alter change edit modify replace reset set switch customising customisation customises customization customizes customizing personalisation personalise personalization personalize activate add begin enable intsall invoke make up setting setup start on turnon unlock midi musical instrument digital interface set-up confg configuration configure define management options settings beeps music noises schemes sounds tones wav beep noise used using devices divises divicse divices"
	],
	[
		"Change text-to-speech settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL %windir%\\system32\\speech\\speechux\\sapi.cpl",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock text to speech tts voice narrator"
	],
	[
		"Print the speech reference card",
		"",
		"T",
		"http://go.microsoft.com/fwlink/?LinkId=619882",
		"adapters adaptor cards out printers printing printner printout print-out pritner card help instructions paper reference sheet recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice"
	],
	[
		"Set up a microphone",
		"",
		"T",
		"%WINDIR%\\system32\\msdt.exe /id SpeechDiagnosticCalibrate /ep ControlPanel /skip true",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock external headset internal jack phone microphones mics mikes recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice used using"
	],
	[
		"Start speech recognition",
		"",
		"T",
		"%windir%\\speech\\common\\sapisvr.exe -SpeechUX",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice"
	],
	[
		"Take speech tutorials",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe %windir%\\system32\\speech\\speechux\\SpeechUX.dll,RunWizard Tutorial",
		"recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice teach training tutorial learn lessons example wizard use how"
	],
	[
		"Train the computer to recognise your voice",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe %windir%\\system32\\speech\\speechux\\SpeechUX.dll,RunWizard UserTraining",
		"accuracy accurately precise precision correct mistakes laptop compuer machine my computer pc personal system customising customisation customises customization customizes customizing personalisation personalise personalization personalize best better easier easy enhance improvement make more most optimal optimise optimize recognise activated commanding dictate dictation recogination recognition recognize recongition reconition speaking speechrecognition speech-recognition talk to understand voice teach training tutorial"
	],
	[
		"Manage offline files",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL %windir%\\system32\\cscui.dll",
		"set-up confg configuration configure define management options personalise personalize up settings setup line offline off-line ofline"
	],
	[
		"Encrypt your offline files",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL %windir%\\system32\\cscui.dll ,2",
		"line offline off-line ofline guard protect secure preserve"
	],
	[
		"Manage disk space used by your offline files",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL %windir%\\system32\\cscui.dll ,1",
		"disc disks drive line offline off-line ofline set-up confg configuration configure define management options personalise personalize up settings setup store space cache room"
	],
	[
		"Adjust the appearance and performance of Windows",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesPerformance.exe",
		"adjust ajust alter change edit modify replace reset set switch optimise crawl degrade faster frozen hanging hung improve inactive optimize performance quicker sluggish speed thrashing too slow unresponsive visuals effects drop shadows smooth scrolling slide fade sliding fading"
	],
	[
		"Allow remote access to your computer",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesRemote.exe",
		"allowed let through permitted laptop compuer machine my computer pc personal system activate add begin enable intsall invoke make up setting setup start on turnon unlock access control ras acess assistance desktop remotely terminal services ts top remote set-up confg configuration configure define management options personalise personalize settings"
	],
	[
		"Allow Remote Assistance invitations to be sent from this computer",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesRemote.exe",
		"allowed let through permitted invitations invite inviting send access control ras acess assistance desktop remotely terminal services ts deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Invite someone to connect to your PC and help you, or offer to help someone else",
		"",
		"T",
		"%SystemRoot%\\system32\\msra.exe",
		"access control ras acess assistance desktop remotely terminal services ts invitations invite inviting send"
	],
	[
		"Change workgroup name",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesComputerName.exe",
		"connect to enter join plug into names rename renaming group workgroup"
	],
	[
		"Check processor speed",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"checking look for scanning verify laptop compuer machine my computer pc personal system 486 agp amd bus chip clock cpu fast intel overclock pentium performance processing processor ram up speedup"
	],
	[
		"Configure advanced user profile properties",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe sysdm.cpl,EditUserProfiles",
		"accounts alias login useraccount useracount users administer configure managed manages managing up setup"
	],
	[
		"Create a restore point",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesProtection.exe",
		"add create make new data recovering recovers recovery points wizard system protection restores systemrestores"
	],
	[
		"Edit environment variables for your account",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe sysdm.cpl,EditEnvironmentVariables",
		"environment variables path envvar adjust ajust alter change edit modify replace reset set switch"
	],
	[
		"Edit the system environment variables",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesAdvanced.exe",
		"environment variables path envvar adjust ajust alter change edit modify replace reset set switch"
	],
	[
		"How to change the size of virtual memory",
		"",
		"T",
		"mshelp://windows/?id=89ca317f-649d-40a6-8934-e5707ee5c4b8",
		"bigger decrease enlarge increase larger resize shape short shrink size skinny smaller taller thin too wider width memory virtualmemory pagefile file"
	],
	[
		"Join a domain",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesComputerName.exe",
		"connect to enter join plug into domain domian server"
	],
	[
		"Rename this computer",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesComputerName.exe",
		"adjust ajust alter change edit modify replace reset set switch laptop compuer machine my computer pc personal system names rename renaming"
	],
	[
		"Create a recovery drive",
		"",
		"T",
		"%windir%\\system32\\RecoveryDrive.exe",
		"add create make new disk drive harddisk hard-disk harddrive hard-drive re-instate re-set recovery recover reinstall factory re-image reimage re-install repair restore reinstate reset wipe rollback fix troubleshoot pc refresh erase format"
	],
	[
		"See the name of this computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"names rename renaming laptop compuer machine my computer pc personal system check list see show viewing"
	],
	[
		"Select users who can use remote desktop",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesRemote.exe",
		"accounts alias login useraccount useracount users allowed let through permitted access permissions priveleges priviledges privileges rights control ras acess assistance desktop remotely terminal services ts top remote"
	],
	[
		"Show how much RAM is on this computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"laptop compuer machine my computer pc personal system amount of many much howmuch gigabytes gigs megabytes megs memory ram automatically autoshow auto-show make visible see show unlock view check list viewing"
	],
	[
		"Show which domain your computer is on",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"domain domian automatically autoshow auto-show make visible see show unlock view check list viewing"
	],
	[
		"Show which operating system your computer is running",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"laptop compuer machine my computer pc personal system operating os release version edition what which"
	],
	[
		"Show which workgroup this computer is on",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"group workgroup automatically autoshow auto-show make visible see show unlock view check list viewing"
	],
	[
		"View advanced system settings",
		"",
		"T",
		"%windir%\\system32\\SystemPropertiesAdvanced.exe",
		"set-up confg configuration configure define management options personalise personalize up settings setup laptop compuer machine my computer pc personal system"
	],
	[
		"View basic information about your computer",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.System",
		"laptop compuer machine my computer pc personal system gigabytes gigs megabytes megs memory ram automatically autoshow auto-show make visible see show unlock view check list viewing"
	],
	[
		"Task Manager",
		"",
		"T",
		"%windir%\\system32\\taskmgr.exe /6",
		"proceses processes task manager mgr manager taskmgr view task application taskman end task close"
	],
	[
		"View system resource usage in Task Manager",
		"",
		"T",
		"%windir%\\system32\\taskmgr.exe /6",
		"disk drive harddisk hard-disk harddrive hard-drive network connections netwerk conection 486 agp amd bus chip clock cpu fast intel overclock pentium performance processing processor ram up speedup memory virtualmemory pagefile file"
	],
	[
		"See which processes start up automatically when you start Windows",
		"",
		"T",
		"%windir%\\system32\\taskmgr.exe /6 /Startup",
		"start-up items up startup programs"
	],
	[
		"Auto-hide the taskbar",
		"Personalization",
		"A",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Taskbar",
		"flicks gestures flik filcks penflick autohide auto-hide automatically hide make invisible set-up confg configuration configure define management options personalise personalize up settings setup autoshow auto-show visible see show unlock view icons area sgtray.exe start tray systemtray systray task toolbar taskbar task-bar bars toolbars notifications notifying notified"
	],
	[
		"Navigation properties",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Options_RunDLL 8",
		"customising customisation customises customization customizes customizing personalisation personalise personalization personalize sign-in applications view navigation desktop mouse start corner corners background windows logo key windows key win winkey sign in signin apps view boot to charm charms powershell command prompt"
	],
	[
		"Group similar windows on the taskbar",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Taskbar",
		"grouping icons area sgtray.exe start tray systemtray systray task toolbar taskbar task-bar bars toolbars vista windows windw 7 notifications notifying notified"
	],
	[
		"Lock or unlock the taskbar",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Taskbar",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff icons area sgtray.exe tray systemtray systray task toolbar taskbar task-bar bars toolbars release version edition what which notifications notifying notified"
	],
	[
		"What's happened to the Quick Launch toolbar?",
		"",
		"T",
		"mshelp://windows/?id=329cf1bc-0f32-41ba-b717-362880a7f6df",
		"icons area sgtray.exe start tray systemtray systray task toolbar taskbar task-bar bars toolbars launch quicklaunch quiklaunch pin notifications notifying notified"
	],
	[
		"Add or remove user accounts",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts /page pageAdminTasks",
		"accounts alias login useraccount useracount users added adding addon add-on adds activate add begin enable intsall invoke make up setting setup start on turnon unlock deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff"
	],
	[
		"Change account type",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts /page pageAdminTasks",
		"accounts alias login useraccount useracount users adjust ajust alter change edit modify replace reset set switch add create make new names rename renaming"
	],
	[
		"Create a password reset disk",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe keymgr.dll,PRShowSaveWizardExW",
		"adjust ajust alter change edit modify replace reset set switch add create make new disc disks drive can't remember don't forget forgotten lost word pass-word passwords passwork passwrod pasword paswrod"
	],
	[
		"Create an account",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts /page pageAdminTasks",
		"accounts alias login useraccount useracount users add create make new"
	],
	[
		"Create standard user account",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts /page pageAdminTasks",
		"accounts alias login useraccount useracount users add create make new activate begin enable intsall invoke up setting setup start on turnon unlock"
	],
	[
		"Edit local users and groups",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\lusrmgr.msc",
		"accounts alias login useraccount useracount users adjust ajust alter change edit modify replace reset set switch"
	],
	[
		"Give administrative rights to a domain user",
		"",
		"T",
		"%windir%\\system32\\netplwiz.exe",
		"accounts alias login useraccount useracount users administrator admins allowed let through permitted add create make new domain domian activate begin enable intsall invoke up setting setup start on turnon unlock access permissions priveleges priviledges privileges rights"
	],
	[
		"Give other users access to this computer",
		"",
		"T",
		"%windir%\\system32\\netplwiz.exe",
		"accounts alias login useraccount useracount users laptop compuer machine my computer pc personal system add create make new access permissions priveleges priviledges privileges rights"
	],
	[
		"How to change your Windows password",
		"",
		"T",
		"mshelp://windows/?id=5c07e067-286d-4b8d-b342-431306e696aa",
		"accounts alias login useraccount useracount users adjust ajust alter change edit modify replace reset set switch add create make new names rename renaming word pass-word passwords passwork passwrod pasword paswrod"
	],
	[
		"Make changes to accounts",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts /page pageAdminTasks",
		"accounts alias login useraccount useracount users adjust ajust alter change edit modify replace reset set switch access permissions priveleges priviledges privileges rights"
	],
	[
		"Manage file encryption certificates",
		"",
		"T",
		"%windir%\\system32\\rekeywiz.exe",
		"certificates keys signatures signed guard protect secure preserve files administer configure managed manages managing up setup"
	],
	[
		"Manage user certificates",
		"",
		"T",
		"%windir%\\system32\\mmc.exe %windir%\\system32\\certmgr.msc",
		"certificate key"
	],
	[
		"Manage network passwords",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.CredentialManager /page ?SelectedVault=CredmanVault",
		"administer configure managed manages managing up setup connections go online intranet lan netowrk networking line on-line www word pass-word passwords passwork passwrod pasword paswrod restricted safety secure securing security sites trusted trusting"
	],
	[
		"Reset Security Policies",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.UserAccounts",
		"policies policys reset security exchange activesync"
	],
	[
		"Allow an app through Windows Firewall",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsFirewall /page PageConfigureApps",
		"allowed let through permitted ecxeption exception exeption virus builtin built-in fire-wall firewalled firwall frewall ifc protection walled from the web inet internet intrnet net online on-line sites pages webpages websites world wide www file applets applications apps aps files programmes programs softare softwares sofware sotfware windos windows restricted safety secure securing security trusted trusting set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Check firewall status",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsFirewall",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock virus builtin built-in fire-wall firewalled firwall frewall ifc protection walled from the web inet internet intrnet net online on-line sites pages webpages websites world wide www deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff restricted safety secure securing security trusted trusting"
	],
	[
		"Manage BitLocker",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.BitLockerDriveEncryption",
		"laptop compuer machine my computer pc personal system disc disks drive locker bitlocker cryptology decrypt encode encryption device encryption disk harddisk hard-disk harddrive hard-drive guard protect secure preserve restricted safety securing security sites trusted trusting devices divises divicse divices"
	],
	[
		"Back up your recovery key",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.BitLockerDriveEncryption",
		"laptop compuer machine my computer pc personal system disc disks drive locker bitlocker cryptology decrypt encode encryption device encryption disk harddisk hard-disk harddrive hard-drive guard protect secure preserve restricted safety securing security sites trusted trusting devices divises divicse divices"
	],
	[
		"Manage Storage Spaces",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.StorageSpaces",
		"disk drive harddisk hard-disk harddrive hard-drive data recovering recovers recovery disc disks storage spaces storage-spaces mirror mirror-space resilient resilient-space"
	],
	[
		"Manage Work Folders",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WorkFolders",
		"files line offline off-line ofline folders subfolders sync-centre synchronise work work-folders work-files company-folders company-files sync-center synchronize"
	],
	[
		"Set flicks to perform certain tasks",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @0,flicks",
		"adjust ajust alter change edit modify replace reset set switch devices divises divicse divices flicks gestures flik filcks penflick hand-writing handwriting write pen stylus handrighting hand-righting penmanship calligraphy set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Change tablet pen settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @0,pen",
		"devices divises divicse divices hand-writing handwriting write pen stylus handrighting hand-righting penmanship calligraphy set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Turn flicks on or off",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @0,flicks",
		"devices divises divicse divices activate add begin enable intsall invoke make up setting setup start on turnon unlock flicks gestures flik filcks penflick hand-writing handwriting write pen stylus handrighting hand-righting penmanship calligraphy set-up confg configuration configure define management options personalise personalize settings"
	],
	[
		"Change touch input settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @0,touch",
		"devices divises divicse divices activate add begin enable intsall invoke make up setting setup start on turnon unlock touch pointer finger pointing cursor mouse"
	],
	[
		"Change multi-touch gesture settings",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @0,touch",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock devices divises divicse divices touch pointer finger pointing cursor mouse"
	],
	[
		"Calibrate the screen for pen or touch input",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @1,general",
		"accuracy accurately precise precision correct mistakes calibrate calibration monitors screens crt displays screeens pen touch"
	],
	[
		"Choose the order of how your screen rotates",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @1,display",
		"adjust ajust alter change edit modify replace reset set switch monitors screens angle landscape orientation portrait rotate rotation crt displays screeens"
	],
	[
		"Set tablet buttons to perform certain tasks",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @1,buttons",
		"butons buttons adjust ajust alter change edit modify replace reset set switch customising customisation customises customization customizes customizing personalisation personalise personalization personalize tablets pc"
	],
	[
		"Specify which hand you write with",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL TabletPC.cpl @1,other",
		"hand-writing handwriting write pen stylus handrighting hand-righting penmanship calligraphy flip handed hander left-handed left-hander lefty reverse right-handed switch set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"Send or receive a file",
		"",
		"T",
		"%windir%\\system32\\rundll32.exe shell32.dll,Control_RunDLL irprops.cpl",
		"infrared infra-red wireless beam send transmit"
	],
	[
		"Change Windows SideShow-compatible device settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsSideShow /page pageChangeSettingsDeviceSelector",
		"devices divises divicse divices ware hardware-general side-show auxiliary display show sideshow change settings gadgets widgets"
	],
	[
		"Change the order of Windows SideShow gadgets",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsSideShow /page pageReorderGadgetsDeviceSelector",
		"devices divises divicse divices gadgets widgets organise organising arrange arranging ordering organize organizing reorder side-show auxiliary display show sideshow alphabetise alphabetize order sorting"
	],
	[
		"Change Windows SideShow settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsSideShow",
		"devices divises divicse divices gadgets widgets deactivate cancel deactivates delete deleting disable disallow exit get rid of prevent remove removing shut down off stop turn turnoff side-show auxiliary display show sideshow"
	],
	[
		"Change PC wake-up settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.WindowsSideShow /page pageAutoWake",
		"automated automatically laptop compuer machine my computer pc personal system side-show auxiliary display show sideshow auto out period timeout timer timing asleep autowake awaken come back from hibernates hibernating hibernation restarts resumes sleeping standby up wakeup wake-up waking"
	],
	[
		"Calibrate display colour",
		"",
		"T",
		"%windir%\\system32\\dccw.exe",
		"adjust ajust alter change edit modify replace reset set switch crt displays monitors screeens screens adapters adaptor cards calibrate calibration brightness contrast hi-contrast high-contrast troubleshooting history problem"
	],
	[
		"Adjust ClearType text",
		"",
		"T",
		"%windir%\\system32\\cttune.exe",
		"faunt fonts type truetype opentype fount dots per inch dpi pixels high reslution resoltion resolution resolutoin rezolution size wizard wizzard personalise personalisation personalize personalization cleartype tuning tuner settings contrast gamma text aliasing anti-aliasing filtering blurry fuzzy typography reading readability easier to easy font clarity color colour enable tweak typeface ppi legible legibility"
	],
	[
		"Connect to a network",
		"",
		"T",
		"ms-availablenetworks:",
		"connect to enter join plug into from the web inet internet intrnet net online on-line sites pages webpages websites world wide www connections go intranet lan netowrk networking line 802.1x wireless wire-less set-up confg configuration configure define management options personalise personalize up settings setup wifi wi-fi wlan aeroplane all on off plane flight radio airplane connection da direct access directaccess mobile phone provider mobile broadband 2g 2.5g 3g 3.5g 4g lte cdma gsm cellphone mbb telco carrier"
	],
	[
		"Set up a connection or network",
		"",
		"T",
		"%windir%\\system32\\xwizard.exe RunWizard {7071ECE0-663B-4bc1-A1FA-B97F3B917C55}",
		"tooth bluetooth blutooth connect to enter join plug into infrared infra-red wireless connections go online intranet lan netowrk networking line on-line www 802.1x wire-less"
	],
	[
		"Identify and repair network problems",
		"",
		"T",
		"%windir%\\system32\\Rundll32.exe ndfapi,NdfRunDllDiagnoseIncident",
		"broken doesn't not working problems won't diagnose diagnostics diagnosis find analyse analyze analysis troubleshooter errors from the web inet internet intrnet net online on-line sites pages webpages websites world wide www connections go intranet lan netowrk networking line corrects fixes repairs wireless ethernet packet dhcp tcp/ip protocol nic cable certificate wifi wi-fi wlan"
	],
	[
		"Manage advanced sharing settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.NetworkAndSharingCenter /page Advanced",
		"sharing shared files network folder target"
	],
	[
		"Set up a dial-up connection",
		"",
		"T",
		"%windir%\\system32\\xwizard.exe RunWizard {7071EC71-663B-4bc1-A1FA-B97F3B917C55}",
		"area code carrier city up dialing dialling dialup dial-up pulse tone touchtone touch-tone activate add begin enable intsall invoke make setting setup start on turnon unlock modem connections go online intranet lan netowrk networking line on-line www set-up confg configuration configure define management options personalise personalize settings"
	],
	[
		"Set up a virtual private network (VPN) connection",
		"",
		"T",
		"%windir%\\system32\\xwizard.exe RunWizard {7071EC75-663B-4bc1-A1FA-B97F3B917C55}",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock connections go online intranet lan netowrk networking line on-line www set-up confg configuration configure define management options personalise personalize settings virtual private network vpn workplace"
	],
	[
		"Set up a broadband connection",
		"",
		"T",
		"%windir%\\system32\\xwizard.exe RunWizard {C03E8585-781E-49a1-8190-CE902D0B2CE7}",
		"pppoe broadband add create make new connections go online intranet lan netowrk networking line on-line www set-up confg configuration configure define management options personalise personalize up settings setup"
	],
	[
		"View network computers and devices",
		"",
		"T",
		"shell:::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}",
		"adapters adaptor cards devices divises divicse divices connections go online intranet lan netowrk networking line on-line www check list see show viewing"
	],
	[
		"View network connections",
		"",
		"T",
		"shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}\\3\\::{7007ACC7-3202-11D1-AAD2-00805FC1270E}",
		"adapters adaptor cards internet protocol ip addresses v6 version 6 ip6 ipv6 tcp/ip tcp\\ip tcpip connections go online intranet lan netowrk networking line on-line www check list see show viewing wireless ethernet packet dhcp nic cable certificate virtual private network vpn workplace"
	],
	[
		"View network status and tasks",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.NetworkAndSharingCenter",
		"connections go online intranet lan netowrk networking line on-line www automatically autoshow auto-show make visible see show unlock view condition running state status check list viewing locations networks"
	],
	[
		"Media streaming options",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.NetworkAndSharingCenter /page ShareMedia",
		"devices divises divicse divices sharing shared files network folder target stream media library options with to mediasharing mediastreaming sharemedia internetsharing"
	],
	[
		"Access RemoteApp and desktops",
		"",
		"T",
		"%windir%\\system32\\xwizard.exe RunWizard {7940ACF8-60BA-4213-A7C3-F3B400EE266D}",
		"activate add begin enable intsall invoke make up setting setup start on turnon unlock set-up confg configuration configure define management options personalise personalize settings remoteapp desktop remotedesktop connection work"
	],
	[
		"Find and fix problems",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Troubleshooting",
		"troubleshoot shoot troubleshooter trubleshoot help solution diagnose diagnostics diagnosis find problems analyse analyze analysis errors"
	],
	[
		"Find and fix audio playback problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id AudioPlaybackDiagnostic -ep ControlPanelSearch",
		"playing sound volume audible speaker headphone stereo listen hear output mp3 player"
	],
	[
		"Find and fix audio recording problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id AudioRecordingDiagnostic -ep ControlPanelSearch",
		"capture sample mix level input sound audible volume stereo tape microphone mike"
	],
	[
		"Perform recommended maintenance tasks automatically",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id MaintenanceDiagnostic -ep ControlPanelSearch",
		"optimise maintain defrag optimize performance operating system os"
	],
	[
		"Find and fix networking and connection problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id NetworkDiagnosticsWeb -ep ControlPanelSearch",
		"sharing shared files network folder target internet wireless ethernet packet dhcp tcp/ip protocol nic cable certificate lan wifi wi-fi wlan"
	],
	[
		"Find and fix printing problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id PrinterDiagnostic -ep ControlPanelSearch",
		"print paper ink toner laser bubble jet cartridge out printers printing printner printout print-out pritner"
	],
	[
		"Troubleshooting History",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.Troubleshooting /page HistoryPage",
		"troubleshooting history problem"
	],
	[
		"Find and fix problems with Windows Search",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id SearchDiagnostic -ep ControlPanelSearch",
		"finder indexing reindex re-index searches searching"
	],
	[
		"Record steps to reproduce a problem",
		"",
		"T",
		"%windir%\\system32\\psr.exe",
		"recorder screen problem capture snipping psr reproduce"
	],
	[
		"Find and fix keyboard problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id KeyboardDiagnostic -ep ControlPanelSearch",
		"input method editor asian input japanese input ime keyboard input"
	],
	[
		"Find and fix bluescreen problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id BlueScreenDiagnostic -ep CortanaBluescreenSearch",
		"bluescreen bluescreen issues bad driver drivers blue screen bug check"
	],
	[
		"Find and fix windows update problems",
		"",
		"T",
		"%windir%\\system32\\msdt.exe -id windowsupdatediagnostic -ep CortanaWUSearch",
		"can't install update windows update cannot update issues with windows update windows update issues wu update fails cant install update windows update error wu error"
	],
	[
		"Change location settings",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.LocationSettings",
		"private privacy secrecy by defaults defualt standard devices divises divicse divices finding locate locating search for is where's the sensors senser gps global positioning systems ambient lighting changes personal information persenal informatien place position positian new network connections netwerk conection sensor definition what a movement movment movemnet motion detection detecting drop locations change another default location geographic geografic geografik latitude laditude longitude longetude address adress region country contry city orientation orientatien orientatian state"
	],
	[
		"Manage Web Credentials",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.CredentialManager",
		"administer configure managed manages managing up setup connections go online intranet lan netowrk networking line on-line www word pass-word passwords passwork passwrod pasword paswrod web stored automatic logon credentials network vault locker"
	],
	[
		"Manage Windows Credentials",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.CredentialManager /page ?SelectedVault=CredmanVault",
		"administer configure managed manages managing up setup connections go online intranet lan netowrk networking line on-line www word pass-word passwords passwork passwrod pasword paswrod web stored automatic logon credentials network vault locker"
	],
	[
		"Back up and Restore (Windows 7)",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.BackupAndRestore",
		"up backing backuo back-up backups back-ups data storage incramental incremental saver shadow copies copy recovering recovers recovery bring back change recover restore rollback points wizard system protection restores systemrestores"
	],
	[
		"Restore data, files or computer from backup (Windows 7)",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.BackupAndRestore",
		"up backing backuo back-up backups back-ups data storage incramental incremental saver shadow copies copy recovering recovers recovery bring back change recover restore rollback points wizard system protection restores systemrestores deleted files get lost missing rescue retrieve return past previous protected time version"
	],
	[
		"Save backup copies of your files with File History",
		"",
		"T",
		"%SystemRoot%\\System32\\control.exe -name Microsoft.FileHistory",
		"up backing backuo back-up backups back-ups data storage incramental incremental saver shadow copies copy bring back change recover restore rollback devices divises divicse divices disk drive harddisk hard-disk harddrive hard-drive guard protect secure preserve sharing shared files network folder target external separate seperate re-instate re-set recovery reinstall factory re-image reimage re-install repair reinstate reset wipe fix troubleshoot pc refresh erase format archive collection keep safe personal retain save library history associate exclude migrate move reconnect recommend retention settings store"
	],
	[
		"Restore your files with File History",
		"",
		"T",
		"%windir%\\system32\\FileHistory.exe",
		"up backing backuo back-up backups back-ups data storage incramental incremental saver shadow copies copy bring back change recover restore rollback deleted files get lost missing rescue retrieve return past previous protected time version archive collection keep safe personal retain save library history"
	]
];
